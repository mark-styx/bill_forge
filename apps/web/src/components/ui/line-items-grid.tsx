'use client';

import { useState } from 'react';
import { InvoiceLineItem } from '@/lib/api';

interface LineItemsGridProps {
  items: InvoiceLineItem[];
  isEditing: boolean;
  onChange: (items: InvoiceLineItem[]) => void;
}

function formatMoney(amount: number): string {
  return '$' + (amount / 100).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function LineItemsGrid({ items, isEditing, onChange }: LineItemsGridProps) {
  const [confirmDeleteIndex, setConfirmDeleteIndex] = useState<number | null>(null);

  const totalAmount = items.reduce((sum, item) => sum + item.amount.amount, 0);

  const handleFieldChange = (index: number, field: keyof InvoiceLineItem, value: string | number) => {
    const updated = items.map((item, i) => {
      if (i !== index) return item;
      if (field === 'quantity') {
        const qty = typeof value === 'string' ? parseFloat(value) || 0 : value;
        const unitPrice = item.unit_price?.amount ?? 0;
        return {
          ...item,
          quantity: qty,
          amount: { ...item.amount, amount: Math.round(qty * unitPrice) },
        };
      }
      if (field === 'unit_price') {
        const priceCents = typeof value === 'string' ? Math.round(parseFloat(value) * 100) || 0 : value;
        const qty = item.quantity ?? 1;
        return {
          ...item,
          unit_price: { ...item.amount, amount: priceCents },
          amount: { ...item.amount, amount: Math.round((item.quantity ?? 1) * priceCents) },
        };
      }
      return { ...item, [field]: value };
    });
    onChange(updated);
  };

  const handleAddLine = () => {
    const newLineNumber = items.length > 0 ? Math.max(...items.map(i => i.line_number)) + 1 : 1;
    const newItem: InvoiceLineItem = {
      id: crypto.randomUUID(),
      line_number: newLineNumber,
      description: '',
      quantity: 1,
      unit_price: { amount: 0, currency: 'USD' },
      amount: { amount: 0, currency: 'USD' },
      gl_code: undefined,
      department: undefined,
      project: undefined,
    };
    onChange([...items, newItem]);
  };

  const handleDelete = (index: number) => {
    if (confirmDeleteIndex === index) {
      onChange(items.filter((_, i) => i !== index));
      setConfirmDeleteIndex(null);
    } else {
      setConfirmDeleteIndex(index);
    }
  };

  if (items.length === 0 && !isEditing) {
    return (
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
        <div className="p-4 border-b border-slate-200 dark:border-slate-700">
          <h2 className="font-semibold text-slate-900 dark:text-white">Line Items</h2>
        </div>
        <div className="p-6">
          <p className="text-slate-500 dark:text-slate-400">No line items</p>
        </div>
      </div>
    );
  }

  const inputClass = 'w-full px-2 py-1 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded focus:outline-none focus:ring-2 focus:ring-blue-500 text-sm';

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
      <div className="p-4 border-b border-slate-200 dark:border-slate-700">
        <h2 className="font-semibold text-slate-900 dark:text-white">Line Items</h2>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-slate-200 dark:border-slate-700">
              <th className="px-4 py-3 text-left font-medium text-slate-500 dark:text-slate-400 w-10">#</th>
              <th className="px-4 py-3 text-left font-medium text-slate-500 dark:text-slate-400">Description</th>
              <th className="px-4 py-3 text-right font-medium text-slate-500 dark:text-slate-400 w-16">Qty</th>
              <th className="px-4 py-3 text-right font-medium text-slate-500 dark:text-slate-400 w-24">Unit Price</th>
              <th className="px-4 py-3 text-right font-medium text-slate-500 dark:text-slate-400 w-28">Amount</th>
              <th className="px-4 py-3 text-left font-medium text-slate-500 dark:text-slate-400 w-28">GL Code</th>
              <th className="px-4 py-3 text-left font-medium text-slate-500 dark:text-slate-400 w-28">Department</th>
              <th className="px-4 py-3 text-left font-medium text-slate-500 dark:text-slate-400 w-28">Project</th>
              {isEditing && <th className="px-4 py-3 w-10"></th>}
            </tr>
          </thead>
          <tbody>
            {items.map((item, index) => (
              <tr key={item.id} className="border-b border-slate-100 dark:border-slate-700/50">
                <td className="px-4 py-2 text-slate-500 dark:text-slate-400">{item.line_number}</td>
                <td className="px-4 py-2">
                  {isEditing ? (
                    <input
                      type="text"
                      value={item.description}
                      onChange={(e) => handleFieldChange(index, 'description', e.target.value)}
                      className={inputClass}
                    />
                  ) : (
                    <span className="text-slate-900 dark:text-white">{item.description || '-'}</span>
                  )}
                </td>
                <td className="px-4 py-2">
                  {isEditing ? (
                    <input
                      type="number"
                      value={item.quantity ?? ''}
                      onChange={(e) => handleFieldChange(index, 'quantity', e.target.value)}
                      className={`${inputClass} text-right`}
                      min="0"
                      step="1"
                    />
                  ) : (
                    <span className="text-slate-900 dark:text-white block text-right">{item.quantity ?? '-'}</span>
                  )}
                </td>
                <td className="px-4 py-2">
                  {isEditing ? (
                    <input
                      type="number"
                      value={item.unit_price ? item.unit_price.amount / 100 : ''}
                      onChange={(e) => handleFieldChange(index, 'unit_price', e.target.value)}
                      className={`${inputClass} text-right`}
                      min="0"
                      step="0.01"
                    />
                  ) : (
                    <span className="text-slate-900 dark:text-white block text-right">
                      {item.unit_price ? formatMoney(item.unit_price.amount) : '-'}
                    </span>
                  )}
                </td>
                <td className="px-4 py-2">
                  <span className="text-slate-900 dark:text-white block text-right font-medium">
                    {formatMoney(item.amount.amount)}
                  </span>
                </td>
                <td className="px-4 py-2">
                  {isEditing ? (
                    <input
                      type="text"
                      value={item.gl_code ?? ''}
                      onChange={(e) => handleFieldChange(index, 'gl_code', e.target.value)}
                      className={inputClass}
                      placeholder="e.g. 6000-100"
                    />
                  ) : (
                    <span className="text-slate-900 dark:text-white">{item.gl_code || '-'}</span>
                  )}
                </td>
                <td className="px-4 py-2">
                  {isEditing ? (
                    <input
                      type="text"
                      value={item.department ?? ''}
                      onChange={(e) => handleFieldChange(index, 'department', e.target.value)}
                      className={inputClass}
                      placeholder="e.g. Operations"
                    />
                  ) : (
                    <span className="text-slate-900 dark:text-white">{item.department || '-'}</span>
                  )}
                </td>
                <td className="px-4 py-2">
                  {isEditing ? (
                    <input
                      type="text"
                      value={item.project ?? ''}
                      onChange={(e) => handleFieldChange(index, 'project', e.target.value)}
                      className={inputClass}
                      placeholder="e.g. PRJ-001"
                    />
                  ) : (
                    <span className="text-slate-900 dark:text-white">{item.project || '-'}</span>
                  )}
                </td>
                {isEditing && (
                  <td className="px-4 py-2">
                    <button
                      type="button"
                      onClick={() => handleDelete(index)}
                      className={`p-1 rounded transition-colors ${
                        confirmDeleteIndex === index
                          ? 'text-red-600 bg-red-50 dark:bg-red-900/20 hover:bg-red-100 dark:hover:bg-red-900/30'
                          : 'text-slate-400 hover:text-red-500'
                      }`}
                      title={confirmDeleteIndex === index ? 'Click again to confirm' : 'Delete line'}
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <polyline points="3 6 5 6 21 6" />
                        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                      </svg>
                    </button>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t border-slate-200 dark:border-slate-700">
              <td colSpan={4} className="px-4 py-3 text-right font-medium text-slate-700 dark:text-slate-300">
                Total
              </td>
              <td className="px-4 py-3 text-right font-bold text-slate-900 dark:text-white">
                {formatMoney(totalAmount)}
              </td>
              <td colSpan={isEditing ? 4 : 3}>
                {isEditing && (
                  <div className="px-4 py-3">
                    <button
                      type="button"
                      onClick={handleAddLine}
                      className="px-3 py-1.5 text-sm bg-blue-50 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400 rounded-lg hover:bg-blue-100 dark:hover:bg-blue-900/50 transition-colors"
                    >
                      + Add Line
                    </button>
                  </div>
                )}
              </td>
            </tr>
          </tfoot>
        </table>
      </div>
    </div>
  );
}
