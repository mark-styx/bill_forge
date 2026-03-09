import http from 'k6/http';
import { check, sleep } from 'k6';

export default function () {
  const res = http.post('http://localhost:3000/upload', JSON.stringify({
    file: 'path/to/large/image.jpg'
  }));

  check(res, {
    'is status 200': (r) => r.status === 200,
    'response time is less than 5s': (r) => r.timings.duration < 5000
  });

  sleep(1);
}