# BillForge Mobile - Offline-first Approval App

Cross-platform (iOS/Android) mobile app built with Expo (React Native + TypeScript) for reviewing and acting on invoice approvals with full offline support.

## Features

- **Offline-first**: Pending approvals are cached locally and render instantly, even with no network connection.
- **Durable action queue**: Approve/reject actions taken offline are persisted and automatically replayed against the backend when connectivity returns.
- **Single codebase**: Builds to both iOS and Android from one Expo project.

## Prerequisites

- Node.js 20+
- pnpm 9+
- Expo CLI (`npx expo install`)
- For iOS: Xcode + CocoaPods
- For Android: Android Studio + Android SDK

## Getting Started

### Install dependencies

From the monorepo root:

```bash
pnpm install
```

### Configure API connection

Set the backend URL and auth credentials in `app.json` under `expo.extra`, or use Expo's environment configuration:

```json
{
  "expo": {
    "extra": {
      "apiBaseUrl": "http://your-backend:8080",
      "jwt": "your-jwt-token",
      "tenantId": "your-tenant-uuid"
    }
  }
}
```

### Run the app

```bash
# Start Expo dev server
pnpm --filter @billforge/mobile start

# Run on iOS simulator
pnpm --filter @billforge/mobile ios

# Run on Android emulator
pnpm --filter @billforge/mobile android
```

### Run tests

```bash
pnpm --filter @billforge/mobile test
```

### Type check

```bash
pnpm --filter @billforge/mobile typecheck
```

## Architecture

### Offline Queue (`src/lib/offline-queue.ts`)

Core offline-first logic, framework-agnostic and fully testable:

- **Cache**: `cacheApprovals()` / `getCachedApprovals()` persist the last-fetched approval list.
- **Action Queue**: `enqueueAction()` appends approve/reject actions with client-generated IDs for idempotency. Optimistically removes items from cache so the UI updates immediately.
- **Flush**: `flushQueue()` replays queued actions in FIFO order. On success, removes from queue. On 409 Conflict (already processed), drops the action. On network error, stops and preserves remaining actions for the next flush.

### API Client (`src/lib/api.ts`)

Typed HTTP client for the backend's `/api/v1/mobile` endpoints:

- `listApprovals()` - GET pending approvals
- `approve(id, comment)` - POST approve an approval
- `reject(id, reason)` - POST reject an approval
- `syncInvoices(since)` - GET delta sync data

Maps non-2xx responses to typed errors (`ConflictError` / `NetworkError`) for the offline queue to branch on.

### App Screen (`App.tsx`)

Single approval-queue screen that:
- Renders cached approvals immediately on load
- Fetches fresh data when online
- Shows an online/offline banner with pending-sync count
- Provides approve/reject buttons with comment/reason prompts
- Auto-flushes the queue on mount and when connectivity is regained

## Backend Endpoints

The app consumes these existing backend endpoints:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/mobile/approvals` | GET | List pending approvals |
| `/api/v1/mobile/approvals/:id/approve` | POST | Approve (body: `{comment}`) |
| `/api/v1/mobile/approvals/:id/reject` | POST | Reject (body: `{reason}`) |
| `/api/v1/mobile/sync/invoices` | GET | Delta sync (query: `?last_sync_at=`) |
