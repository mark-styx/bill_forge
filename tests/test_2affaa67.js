// performance/tests/invoice.js
import http from 'k6/http';

export default function() {
    const res = http.post('http://localhost:8080/api/invoices', JSON.stringify({
        id: 1,
        amount: 250.00,
        description: 'Sample Invoice'
    }), { headers: { 'Content-Type': 'application/json' } });

    if (res.status !== 201) {
        console.error('Invoice creation failed:', res);
    }
}