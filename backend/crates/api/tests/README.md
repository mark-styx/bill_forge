# BillForge Test Suite

This directory contains comprehensive tests for the BillForge API.

## Test Structure

```
backend/crates/api/tests/
├── integration_tests.rs      # API integration tests (auth, validation)
├── dashboard_tests.rs        # Dashboard metrics tests
├── quickbooks_tests.rs       # QuickBooks integration tests
├── email_action_tests.rs     # Email action token tests
└── load_test.rs              # Performance and load tests
```

## Running Tests

### Unit Tests
```bash
# Run all unit tests
cargo test --lib

# Run specific test file
cargo test --test dashboard_tests

# Run with verbose output
cargo test -- --nocapture
```

### Integration Tests
```bash
# Run all integration tests
cargo test --test integration_tests

# Run multi-tenant isolation tests (requires PostgreSQL)
cargo test --test multi_tenant_integration -- --ignored
```

### Load Tests
```bash
# Start the server first
cargo run --release

# In another terminal, run load tests
cargo test --test load_test -- --ignored --nocapture
```

## Test Coverage

### Dashboard Tests (`dashboard_tests.rs`)
- Dashboard metrics structure validation
- Serialization/deserialization tests
- Authentication requirement tests
- Aggregation logic tests

### QuickBooks Tests (`quickbooks_tests.rs`)
- OAuth flow authentication tests
- Vendor/account data structure tests
- Bill creation validation
- Token structure tests

### Email Action Tests (`email_action_tests.rs`)
- Token validation tests
- Signature verification tests
- Expiration handling tests
- URL generation tests

### Integration Tests (`integration_tests.rs`)
- Health check endpoints
- Authentication/authorization
- Input validation
- Error handling
- CORS configuration

### Load Tests (`load_test.rs`)
- Health endpoint benchmarking
- Liveness endpoint benchmarking
- Concurrent request handling
- Performance regression testing

## Test Database

Integration tests use an in-memory SQLite database for speed:
```bash
DATABASE_URL=sqlite://:memory:
```

For PostgreSQL-dependent tests (multi-tenant):
```bash
# Set up test database
export TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5432/billforge_test"
export TEST_TENANT_DB_TEMPLATE="postgres://postgres:postgres@localhost:5432/{database}"

# Create test database
createdb billforge_test

# Run tests
cargo test --test multi_tenant_integration -- --ignored
```

## Performance Benchmarks

Load tests verify SLA compliance:

| Endpoint | P95 Latency Target | P99 Latency Target |
|----------|-------------------|-------------------|
| Health Check | < 10ms | < 20ms |
| Liveness | < 10ms | < 15ms |
| Dashboard Metrics | < 100ms (auth failure) | < 150ms |
| Concurrent Requests | 50 req < 5s | - |

## Continuous Integration

Tests run automatically in CI:

```yaml
# .github/workflows/ci.yml
- name: Run tests
  run: |
    cargo test --lib
    cargo test --test integration_tests
    cargo test --test dashboard_tests
    cargo test --test quickbooks_tests
    cargo test --test email_action_tests
```

## Test Data

Mock data for testing:

```rust
// Dashboard metrics
total_invoices: 1250
pending_ocr: 15
approved: 1080
trend_vs_last_month: 12.5

// QuickBooks vendor
id: "1"
display_name: "Acme Corp"
email: "contact@acme.com"

// Email action token
action: ApproveInvoice
expires_at: Utc::now() + Duration::hours(72)
```

## Debugging Failed Tests

### Enable verbose logging
```bash
RUST_LOG=debug cargo test --test integration_tests -- --nocapture
```

### Run single test
```bash
cargo test test_health_endpoint_returns_200 -- --exact --nocapture
```

### Check test coverage
```bash
cargo tarpaulin --out Html --output-dir target/coverage
```

## Test Best Practices

1. **Isolation**: Each test should be independent
2. **Speed**: Unit tests should run in <1ms
3. **Clarity**: Test names describe what they verify
4. **Coverage**: Aim for >80% line coverage
5. **Realism**: Integration tests use realistic data

## Adding New Tests

1. Create test file in `tests/` directory
2. Follow naming convention: `*_tests.rs`
3. Use helper functions for common setup
4. Document test purpose in comments
5. Run all tests before committing

## Test Dependencies

```toml
[dev-dependencies]
tokio-test = "0.4"
mockall = "0.12"
wiremock = "0.5"
reqwest = { version = "0.11", features = ["json"] }
```

## Troubleshooting

### Tests hang
- Check for database connection issues
- Verify no port conflicts
- Use `--test-threads=1` for sequential execution

### Intermittent failures
- Check for race conditions
- Ensure proper test isolation
- Verify mock data consistency

### Low coverage
- Add edge case tests
- Test error paths
- Verify all branches covered
