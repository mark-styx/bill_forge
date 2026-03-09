import React from 'react';
import { render, fireEvent } from '@testing-library/react';
import ResultDisplay from './ResultDisplay';

describe('ResultDisplay', () => {
  it('displays OCR results correctly', () => {
    const results = {
      Google: "{'text': 'Sample text'}",
      Microsoft: "{'text': 'Another sample text'}"
    };
    const { getByText } = render(<ResultDisplay results={results} />);
    expect(getByText(/Sample text/i)).toBeInTheDocument();
    expect(getByText(/Another sample text/i)).toBeInTheDocument();
  });

  it('allows manual input of OCR results', () => {
    const { getByPlaceholderText, getByText } = render(<ResultDisplay results={{}} />);
    fireEvent.change(getByPlaceholderText(/Enter manual result here/i), { target: { value: "{'text': 'Manual text'}" } });
    expect(getByText(/manual text/i)).toBeInTheDocument();
  });
});