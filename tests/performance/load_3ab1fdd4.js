import http from 'k6/http';
import { sleep } from 'k6';

export default function () {
  // Simulate user load
  http.post('http://localhost:8080/upload', JSON.stringify({ file: 'path/to/document.pdf' }), {
    headers: {
      'Content-Type': 'application/json',
    },
  });

  sleep(1); // Sleep for 1 second to simulate time between requests
}