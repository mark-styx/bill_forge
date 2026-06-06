'use client';

import { useState, useRef, useEffect } from 'react';
import { askDocument } from '@/lib/documentQa';
import type { Citation, QaMessage } from '@/types/documentQa';

interface DocumentQAPanelProps {
  documentId: string | undefined;
  onCitationClick?: (citation: Citation) => void;
}

export function DocumentQAPanel({ documentId, onCitationClick }: DocumentQAPanelProps) {
  const [messages, setMessages] = useState<QaMessage[]>([]);
  const [question, setQuestion] = useState('');
  const [loading, setLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSubmit = async () => {
    if (!question.trim() || !documentId || loading) return;

    const userMessage: QaMessage = { role: 'user', text: question.trim() };
    setMessages((prev) => [...prev, userMessage]);
    setQuestion('');
    setLoading(true);

    try {
      const response = await askDocument(documentId, userMessage.text);
      const assistantMessage: QaMessage = {
        role: 'assistant',
        text: response.answer,
        citations: response.citations,
      };
      setMessages((prev) => [...prev, assistantMessage]);
    } catch (err: any) {
      const errorMessage: QaMessage = {
        role: 'assistant',
        text: `Error: ${err.message || 'Failed to get answer'}`,
      };
      setMessages((prev) => [...prev, errorMessage]);
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <div className="flex flex-col h-full bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
      {/* Header */}
      <div className="p-3 border-b border-slate-200 dark:border-slate-700">
        <h3 className="font-semibold text-sm text-slate-900 dark:text-white">
          Ask about this document
        </h3>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3 min-h-0">
        {messages.length === 0 && (
          <p className="text-xs text-slate-400 dark:text-slate-500 text-center mt-8">
            Ask a question about this invoice document
          </p>
        )}
        {messages.map((msg, i) => (
          <div
            key={i}
            className={`text-sm ${
              msg.role === 'user'
                ? 'text-right'
                : 'text-left'
            }`}
          >
            <div
              className={`inline-block max-w-[90%] rounded-lg px-3 py-2 ${
                msg.role === 'user'
                  ? 'bg-blue-500 text-white'
                  : 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
              }`}
            >
              <p className="whitespace-pre-wrap">{msg.text}</p>
              {msg.citations && msg.citations.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-1">
                  {msg.citations.map((c) => (
                    <button
                      key={c.id}
                      onClick={() => onCitationClick?.(c)}
                      className="inline-flex items-center px-2 py-0.5 text-xs font-medium rounded bg-yellow-200 dark:bg-yellow-700 text-yellow-800 dark:text-yellow-100 hover:bg-yellow-300 dark:hover:bg-yellow-600 transition-colors"
                      title={`Page ${c.page}: ${c.quote}`}
                    >
                      [#{c.id}]
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>
        ))}
        {loading && (
          <div className="text-left">
            <div className="inline-block rounded-lg px-3 py-2 bg-slate-100 dark:bg-slate-700">
              <div className="flex space-x-1">
                <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
              </div>
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div className="p-3 border-t border-slate-200 dark:border-slate-700">
        <div className="flex gap-2">
          <textarea
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask a question..."
            disabled={!documentId || loading}
            rows={2}
            className="flex-1 px-3 py-2 text-sm bg-slate-50 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none disabled:opacity-50"
          />
          <button
            onClick={handleSubmit}
            disabled={!question.trim() || !documentId || loading}
            className="px-4 py-2 bg-blue-500 text-white text-sm rounded-lg hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed self-end"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
