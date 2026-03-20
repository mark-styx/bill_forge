# BillForge Polish Plan — Customer Demo Ready

## Current State Assessment

### Landing Page (apps/web/src/app/(marketing)/page.tsx)
- EXISTS but not accessible from root URL — root redirects to /login
- Marketing layout exists at (marketing)/layout.tsx
- The page has: features grid, pricing tiers, testimonials, stats
- NOT connected to the main navigation flow

### Login Page (apps/web/src/app/login/page.tsx)
- Looks decent visually (blue gradient left, white form right)
- PROBLEMS for demos:
  1. Shows "Product Configuration" dropdown with "Full Platform / 0 modules" — confusing
  2. Shows raw "Tenant ID" field with a UUID — prospects don't care
  3. Shows "Color Theme" swatches — dev feature
  4. Shows "Sandbox Mode" section — dev feature
  5. "Trusted by 500+ finance teams" — premature, don't claim what isn't true
  6. Pre-filled with admin@sandbox.local — OK for demo but should look cleaner

### Root Page (apps/web/src/app/page.tsx)
- Just a redirect: if authenticated → /dashboard, else → /login
- Shows loading spinner during redirect — not good

### Dashboard (apps/web/src/app/(dashboard)/dashboard/page.tsx)
- 513 lines — substantial
- Can't test without backend running

### Key Numbers
- 27 page routes
- 58 UI components 
- 102 total tsx/ts files

## Priority Fixes

### P0 — Login Page Polish
- Hide: Product Configuration, Tenant ID, Color Theme, Sandbox Mode sections
- Keep it dead simple: Email + Password + Sign In button
- Fix "0 modules" display
- Remove "Trusted by 500+ finance teams" or replace with something honest
- Add a subtle "Try the demo" or "Explore the platform" CTA

### P1 — Landing Page 
- Make root URL serve the marketing page for unauthenticated users
- OR add a clear navigation path from login to marketing page
- Review the marketing copy against northstar.md positioning
- Ensure pricing aligns with mid-market positioning ($99-$999/mo range from northstar)

### P2 — Demo Flow
- Ensure seed data in state.rs creates a compelling narrative
- The 16 vendors, 30+ invoices should tell a story
- Dashboard KPIs should show interesting numbers
