package main

import (
	"bytes"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math"
	"net/http"
	"os"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type result struct {
	latency  float64
	status   int
	err      string
	respSize int64
}

type summary struct {
	URL             string   `json:"url"`
	Requests        int      `json:"requests"`
	Concurrency     int      `json:"concurrency"`
	WallSeconds     float64  `json:"wall_seconds"`
	RequestsPerSec  float64  `json:"requests_per_sec"`
	Success         int      `json:"success"`
	Errors          int      `json:"errors"`
	ErrorRate       float64  `json:"error_rate"`
	AverageMS       float64  `json:"avg_ms"`
	P50MS           float64  `json:"p50_ms"`
	P95MS           float64  `json:"p95_ms"`
	P99MS           float64  `json:"p99_ms"`
	MaxMS           float64  `json:"max_ms"`
	MinMS           float64  `json:"min_ms"`
	ResponseBytes   int64    `json:"response_bytes"`
	StatusBreakdown []string `json:"status_breakdown"`
	SampleErrors    []string `json:"sample_errors,omitempty"`
}

func main() {
	var (
		url         = flag.String("url", "", "target url")
		method      = flag.String("method", "POST", "HTTP method")
		requests    = flag.Int("requests", 1000, "number of requests")
		concurrency = flag.Int("concurrency", 32, "concurrency")
		timeout     = flag.Duration("timeout", 30*time.Second, "request timeout")
		body        = flag.String("body", "", "request body")
		headerArgs  multiFlag
	)
	flag.Var(&headerArgs, "header", "request header Key: Value")
	flag.Parse()

	if strings.TrimSpace(*url) == "" {
		fmt.Fprintln(os.Stderr, "missing --url")
		os.Exit(2)
	}
	if *requests <= 0 || *concurrency <= 0 {
		fmt.Fprintln(os.Stderr, "--requests and --concurrency must be > 0")
		os.Exit(2)
	}

	headers := make(http.Header)
	for _, raw := range headerArgs {
		parts := strings.SplitN(raw, ":", 2)
		if len(parts) != 2 {
			fmt.Fprintf(os.Stderr, "invalid header: %s\n", raw)
			os.Exit(2)
		}
		headers.Add(strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1]))
	}

	bodyBytes := []byte(*body)
	results := make([]result, *requests)
	var next int64
	var wg sync.WaitGroup

	client := &http.Client{
		Timeout: *timeout,
		Transport: &http.Transport{
			Proxy:               http.ProxyFromEnvironment,
			MaxIdleConns:        *concurrency * 2,
			MaxIdleConnsPerHost: *concurrency * 2,
			MaxConnsPerHost:     *concurrency * 2,
			IdleConnTimeout:     90 * time.Second,
			DisableCompression:  true,
		},
	}

	start := time.Now()
	for i := 0; i < *concurrency; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				idx := int(atomic.AddInt64(&next, 1)) - 1
				if idx >= *requests {
					return
				}
				results[idx] = performRequest(client, *method, *url, headers, bodyBytes)
			}
		}()
	}
	wg.Wait()
	wall := time.Since(start).Seconds()

	out := buildSummary(*url, *requests, *concurrency, wall, results)
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(out); err != nil {
		fmt.Fprintf(os.Stderr, "encode summary: %v\n", err)
		os.Exit(1)
	}
}

func performRequest(client *http.Client, method, url string, headers http.Header, body []byte) result {
	start := time.Now()
	req, err := http.NewRequestWithContext(context.Background(), method, url, bytes.NewReader(body))
	if err != nil {
		return result{err: err.Error()}
	}
	req.Header = headers.Clone()
	resp, err := client.Do(req)
	if err != nil {
		return result{latency: time.Since(start).Seconds() * 1000, err: err.Error()}
	}
	defer resp.Body.Close()
	n, _ := io.Copy(io.Discard, resp.Body)
	return result{
		latency:  time.Since(start).Seconds() * 1000,
		status:   resp.StatusCode,
		respSize: n,
	}
}

func buildSummary(url string, requests, concurrency int, wall float64, results []result) summary {
	latencies := make([]float64, 0, len(results))
	statusCount := map[int]int{}
	sampleErrors := make([]string, 0, 3)
	success := 0
	var totalLatency float64
	var respBytes int64

	for _, item := range results {
		if item.latency > 0 {
			latencies = append(latencies, item.latency)
			totalLatency += item.latency
		}
		if item.status >= 200 && item.status < 300 && item.err == "" {
			success++
		}
		if item.status > 0 {
			statusCount[item.status]++
		}
		if item.err != "" && len(sampleErrors) < 3 {
			sampleErrors = append(sampleErrors, item.err)
		}
		respBytes += item.respSize
	}

	sort.Float64s(latencies)
	errorCount := requests - success
	avg := 0.0
	if len(latencies) > 0 {
		avg = totalLatency / float64(len(latencies))
	}

	breakdown := make([]string, 0, len(statusCount))
	statuses := make([]int, 0, len(statusCount))
	for code := range statusCount {
		statuses = append(statuses, code)
	}
	sort.Ints(statuses)
	for _, code := range statuses {
		breakdown = append(breakdown, fmt.Sprintf("%d=%d", code, statusCount[code]))
	}

	return summary{
		URL:             url,
		Requests:        requests,
		Concurrency:     concurrency,
		WallSeconds:     wall,
		RequestsPerSec:  safeDiv(float64(requests), wall),
		Success:         success,
		Errors:          errorCount,
		ErrorRate:       safeDiv(float64(errorCount), float64(requests)),
		AverageMS:       avg,
		P50MS:           percentile(latencies, 0.50),
		P95MS:           percentile(latencies, 0.95),
		P99MS:           percentile(latencies, 0.99),
		MaxMS:           maxFloat(latencies),
		MinMS:           minFloat(latencies),
		ResponseBytes:   respBytes,
		StatusBreakdown: breakdown,
		SampleErrors:    sampleErrors,
	}
}

func percentile(sorted []float64, p float64) float64 {
	if len(sorted) == 0 {
		return 0
	}
	if len(sorted) == 1 {
		return sorted[0]
	}
	if p <= 0 {
		return sorted[0]
	}
	if p >= 1 {
		return sorted[len(sorted)-1]
	}
	pos := p * float64(len(sorted)-1)
	lower := int(math.Floor(pos))
	upper := int(math.Ceil(pos))
	if lower == upper {
		return sorted[lower]
	}
	weight := pos - float64(lower)
	return sorted[lower] + (sorted[upper]-sorted[lower])*weight
}

func safeDiv(a, b float64) float64 {
	if b == 0 {
		return 0
	}
	return a / b
}

func maxFloat(items []float64) float64 {
	if len(items) == 0 {
		return 0
	}
	return items[len(items)-1]
}

func minFloat(items []float64) float64 {
	if len(items) == 0 {
		return 0
	}
	return items[0]
}

type multiFlag []string

func (m *multiFlag) String() string {
	return strings.Join(*m, ",")
}

func (m *multiFlag) Set(value string) error {
	*m = append(*m, value)
	return nil
}
