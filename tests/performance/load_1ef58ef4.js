import http from 'k6/http';
import { check, sleep } from 'k6';

export default function() {
  const server_url = 'http://localhost:8000';
  const content = fs.readFileSync('path/to/sample/invoice.pdf');

  let response = http.post(`${server_url}/ocr`, content, {
    headers: {
      'Content-Type': 'application/pdf'
    }
  });

  check(response, {
    'Status is 200': (r) => r.status === 200,
    'Response time is less than 500ms': (r) => r.timings.duration < 500
  });
  
  sleep(1);
}