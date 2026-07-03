# WeDevs VPS Operations

这份文档记录当前 `sub2api` 线上 VPS 的连接方式、部署目录、数据位置，以及后续更新时的标准重部署流程。

## VPS

- Host: `107.172.252.250`
- User: `root`
- SSH:

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing
```

## Current Layout

- Source repo: `/root/workspaces/sub2api`
- Deploy dir: `/opt/sub2api-official`
- Runtime: `docker compose`
- App bind: `127.0.0.1:18080 -> container:8080`
- Data dirs:
  - `/opt/sub2api-official/data`
  - `/opt/sub2api-official/postgres_data`
  - `/opt/sub2api-official/redis_data`

Important:

- Do not delete the three data directories above during redeploy.
- Do not commit `.env` secrets back into git.
- The deployment currently uses a fixed image tag in compose, so redeploy is done by rebuilding the same tag and recreating only the `sub2api` container.

## Safe Redeploy

Assumption:

- New code has already been pushed to `origin`.
- Current deploy branch is `origin/sync/upstream-2026-04-09`.

### 1. Check current status

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing '
cd /opt/sub2api-official &&
docker compose ps &&
df -h / &&
docker system df
'
```

### 2. Clean unused Docker history first

This reclaims old images and stopped containers without touching live containers or bind-mounted data.

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing '
docker image prune -af &&
docker container prune -f &&
df -h / &&
docker system df
'
```

### 3. Prepare build source from the target branch

Use a temporary build directory instead of switching the working tree in `/root/workspaces/sub2api`.

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing 'bash -se' <<'"'"'EOF'"'"'
set -euo pipefail
REPO=/root/workspaces/sub2api
BUILD_DIR=/root/workspaces/.codex-deploy/sub2api-release

cd "$REPO"
git fetch origin sync/upstream-2026-04-09
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"
git archive --format=tar FETCH_HEAD | tar -C "$BUILD_DIR" -xf -
EOF
```

### 4. If VPS memory is tight, add temporary swap

This VPS has low memory. Frontend production build may fail with exit code `137` or `JavaScript heap out of memory` unless temporary swap is added.

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing 'bash -se' <<'"'"'EOF'"'"'
set -euo pipefail
TEMP_SWAP=/swapfile_build

if [ ! -f "$TEMP_SWAP" ]; then
  fallocate -l 2G "$TEMP_SWAP"
  chmod 600 "$TEMP_SWAP"
  mkswap "$TEMP_SWAP"
fi

swapon "$TEMP_SWAP" || true
free -h
swapon --show
EOF
```

### 5. Increase Node heap only for the temporary build

This change is applied only to the temporary build directory, not to the repository.

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing '
sed -i '"'"'"'"'"'22i ENV NODE_OPTIONS=--max_old_space_size=2048'"'"'"'"'"' /root/workspaces/.codex-deploy/sub2api-release/Dockerfile
'
```

### 6. Backup current image tag, build new image, recreate app container

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing 'bash -se' <<'"'"'EOF'"'"'
set -euo pipefail

BUILD_DIR=/root/workspaces/.codex-deploy/sub2api-release
DEPLOY_DIR=/opt/sub2api-official
LIVE_TAG=sub2api:20260322-usagecompat
BACKUP_TAG=sub2api:backup-$(date +%Y%m%d-%H%M%S)

docker tag "$LIVE_TAG" "$BACKUP_TAG"
docker build -t "$LIVE_TAG" "$BUILD_DIR"

cd "$DEPLOY_DIR"
docker compose up -d --no-deps --force-recreate sub2api
docker compose ps
EOF
```

### 7. Verify

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing '
curl -fsS http://127.0.0.1:18080/health &&
echo &&
cd /opt/sub2api-official &&
docker compose ps &&
docker logs --tail 80 sub2api
'
```

### 8. Cleanup temporary swap and build directory

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing 'bash -se' <<'"'"'EOF'"'"'
set -euo pipefail
TEMP_SWAP=/swapfile_build
BUILD_DIR=/root/workspaces/.codex-deploy/sub2api-release

docker image prune -f || true
swapoff "$TEMP_SWAP" || true
rm -f "$TEMP_SWAP"
rm -rf "$BUILD_DIR"

free -h
swapon --show
df -h /
docker system df
EOF
```

## Rollback

If the new container starts but is unhealthy, retag the backup image to the live tag and recreate only the app container:

```bash
ssh root@107.172.252.250 -o IdentitiesOnly=yes -i ~/.ssh/colocorssing 'bash -se' <<'"'"'EOF'"'"'
set -euo pipefail
DEPLOY_DIR=/opt/sub2api-official
BACKUP_TAG=<replace-with-backup-tag>
LIVE_TAG=sub2api:20260322-usagecompat

docker tag "$BACKUP_TAG" "$LIVE_TAG"
cd "$DEPLOY_DIR"
docker compose up -d --no-deps --force-recreate sub2api
docker compose ps
curl -fsS http://127.0.0.1:18080/health
EOF
```

## Notes

- Current online service is not managed by `systemd`; it is `docker compose`.
- The biggest disk consumer has been old Docker images, not the bind-mounted business data.
- If later migrating to `systemd`, keep PostgreSQL and Redis data migration separate from the app switch. Do not mix database migration with a routine code deploy.
