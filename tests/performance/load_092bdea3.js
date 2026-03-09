import http from 'k6';
import { check, group, sleep } from 'k6';

export default function () {
  let users = [10, 50, 100, 200, 300, 400, 500];

  for (let i = 0; i < users.length; i++) {
    group(`Load ${users[i]} concurrent users`, function () {
      let dataPoints = [];
      for (let j = 0; j < 100; j++) { // Number of requests per user
        http.get('http://localhost:3000/api/match', {
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ vendorData: '{ "name": "Example Vendor", "address": "123 Example St", "contact": { "email": "info@example.com", "phone": "555-1234" } }' }),
        });
        sleep(0.1); // Delay between requests
      }

      let responseTimes = http.get('http://localhost:3000/api/response-times').json();
      dataPoints.push({ users, time: responseTimes.averageResponseTime });

      check(responseTimes, {
        'response time is less than 3 seconds': (r) => r.averageResponseTime < 3000,
      });
    });

    sleep(5); // Delay between load tests
  }
}