package main

import "testing"

func TestPercentileInterpolates(t *testing.T) {
	input := []float64{10, 20, 30, 40}
	if got := percentile(input, 0.50); got != 25 {
		t.Fatalf("percentile p50 = %v, want 25", got)
	}
	if got := percentile(input, 0.95); got != 38.5 {
		t.Fatalf("percentile p95 = %v, want 38.5", got)
	}
}

func TestBuildSummaryCountsSuccessAndErrors(t *testing.T) {
	results := []result{
		{latency: 10, status: 200, respSize: 10},
		{latency: 20, status: 200, respSize: 15},
		{latency: 30, status: 503, respSize: 5},
		{latency: 40, err: "boom"},
	}
	s := buildSummary("http://example", 4, 2, 2, results)
	if s.Success != 2 {
		t.Fatalf("success = %d, want 2", s.Success)
	}
	if s.Errors != 2 {
		t.Fatalf("errors = %d, want 2", s.Errors)
	}
	if s.RequestsPerSec != 2 {
		t.Fatalf("rps = %v, want 2", s.RequestsPerSec)
	}
	if s.ResponseBytes != 30 {
		t.Fatalf("response bytes = %d, want 30", s.ResponseBytes)
	}
	if len(s.StatusBreakdown) != 2 {
		t.Fatalf("status breakdown len = %d, want 2", len(s.StatusBreakdown))
	}
}
