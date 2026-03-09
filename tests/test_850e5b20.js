import http from 'k6';
import { check } from 'k6';

export const options = {
  scenarios: {
    stress_test: {
      executor: 'per-vu-iterations',
      vus: 1000,
      iterations: 1000,
      maxDuration: '5m'
    }
  }
};

export default function() {
  const res = http.post('http://localhost:3000/upload', JSON.stringify({ file: 'path/to/sample_invoice.png' }));
  
  check(res, {
    'status is 200': (r) => r.status === 200,
    'response time < 500ms': (r) => r.timings.duration < 500,
  });
}