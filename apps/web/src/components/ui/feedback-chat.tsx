'use client';

import * as React from 'react';
import { useState, useEffect, useRef } from 'react';
import { cn } from '@/lib/utils';
import { feedbackApi, FeedbackEntry } from '@/lib/api';
import { usePathname } from 'next/navigation';
import { MessageCircle, X, Send, ChevronDown, Tag } from 'lucide-react';

const CATEGORIES = [
  { value: 'general', label: 'General' },
  { value: 'bug', label: 'Bug Report' },
  { value: 'feature', label: 'Feature Request' },
  { value: 'ux', label: 'UX/Design' },
  { value: 'performance', label: 'Performance' },
];

export function FeedbackChat() {
  const [isOpen, setIsOpen] = useState(false);
  const [message, setMessage] = useState('');
  const [category, setCategory] = useState('general');
  const [showCategories, setShowCategories] = useState(false);
  const [entries, setEntries] = useState<FeedbackEntry[]>([]);
  const [sending, setSending] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const pathname = usePathname();

  // Load feedback history when panel opens
  useEffect(() => {
    if (isOpen) {
      loadHistory();
      // Focus input after animation
      setTimeout(() => inputRef.current?.focus(), 300);
    }
  }, [isOpen]);

  // Scroll to bottom when new entries appear
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries]);

  // Close on Escape
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        setIsOpen(false);
      }
    };
    document.addEventListener('keydown', handleKey);
    return () => document.removeEventListener('keydown', handleKey);
  }, [isOpen]);

  async function loadHistory() {
    setLoadingHistory(true);
    try {
      const data = await feedbackApi.list();
      setEntries(Array.isArray(data) ? data : []);
    } catch {
      // Silently fail - empty state is fine
    } finally {
      setLoadingHistory(false);
    }
  }

  async function handleSubmit(e?: React.FormEvent) {
    e?.preventDefault();
    const trimmed = message.trim();
    if (!trimmed || sending) return;

    setSending(true);
    try {
      const entry = await feedbackApi.submit({
        message: trimmed,
        category,
        page: pathname,
      });
      setEntries((prev) => [...prev, entry]);
      setMessage('');
      setCategory('general');
    } catch {
      // Could show error toast, but keep it simple
    } finally {
      setSending(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  }

  const selectedCategory = CATEGORIES.find((c) => c.value === category);

  return (
    <>
      {/* Floating trigger button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className={cn(
          'fixed bottom-6 right-6 z-50 w-12 h-12 rounded-full shadow-lg flex items-center justify-center transition-all duration-300',
          'hover:scale-110 active:scale-95',
          isOpen
            ? 'bg-muted text-muted-foreground rotate-0'
            : 'bg-primary text-primary-foreground'
        )}
        aria-label={isOpen ? 'Close feedback' : 'Send feedback'}
      >
        {isOpen ? <X className="w-5 h-5" /> : <MessageCircle className="w-5 h-5" />}
      </button>

      {/* Chat panel */}
      <div
        className={cn(
          'fixed bottom-20 right-6 z-50 w-96 bg-card border border-border rounded-2xl shadow-2xl flex flex-col transition-all duration-300 origin-bottom-right',
          isOpen
            ? 'opacity-100 scale-100 translate-y-0'
            : 'opacity-0 scale-95 translate-y-4 pointer-events-none'
        )}
        style={{ maxHeight: 'calc(100vh - 8rem)' }}
      >
        {/* Header */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-border bg-secondary/30 rounded-t-2xl">
          <div className="w-8 h-8 rounded-lg bg-primary/10 flex items-center justify-center">
            <MessageCircle className="w-4 h-4 text-primary" />
          </div>
          <div className="flex-1">
            <h3 className="text-sm font-semibold text-foreground">Platform Feedback</h3>
            <p className="text-xs text-muted-foreground">Help us improve BillForge</p>
          </div>
          <button
            onClick={() => setIsOpen(false)}
            className="p-1 rounded-lg hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Messages area */}
        <div ref={scrollRef} className="flex-1 overflow-y-auto p-4 space-y-3 min-h-[200px] max-h-[400px]">
          {loadingHistory ? (
            <div className="flex items-center justify-center py-8 text-muted-foreground">
              <div className="w-5 h-5 border-2 border-primary/30 border-t-primary rounded-full animate-spin" />
            </div>
          ) : entries.length === 0 ? (
            <div className="text-center py-8">
              <MessageCircle className="w-10 h-10 text-muted-foreground/30 mx-auto mb-3" />
              <p className="text-sm text-muted-foreground">No feedback yet</p>
              <p className="text-xs text-muted-foreground/70 mt-1">
                Share your thoughts about the platform
              </p>
            </div>
          ) : (
            entries.map((entry) => (
              <div key={entry.id} className="group">
                <div className="bg-primary/5 border border-primary/10 rounded-xl px-3 py-2">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="text-xs font-medium text-foreground">{entry.user_name}</span>
                    <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-secondary text-muted-foreground">
                      {entry.category}
                    </span>
                  </div>
                  <p className="text-sm text-foreground/90 whitespace-pre-wrap">{entry.message}</p>
                  <div className="flex items-center gap-2 mt-1.5">
                    <span className="text-[10px] text-muted-foreground">
                      {new Date(entry.timestamp).toLocaleString()}
                    </span>
                    {entry.page && (
                      <span className="text-[10px] text-muted-foreground/60 truncate max-w-[150px]">
                        {entry.page}
                      </span>
                    )}
                  </div>
                </div>
              </div>
            ))
          )}
        </div>

        {/* Input area */}
        <div className="border-t border-border p-3">
          {/* Category selector */}
          <div className="relative mb-2">
            <button
              onClick={() => setShowCategories(!showCategories)}
              className="flex items-center gap-1.5 px-2 py-1 rounded-lg text-xs bg-secondary/60 hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors"
            >
              <Tag className="w-3 h-3" />
              {selectedCategory?.label}
              <ChevronDown className={cn('w-3 h-3 transition-transform', showCategories && 'rotate-180')} />
            </button>
            {showCategories && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setShowCategories(false)} />
                <div className="absolute bottom-full left-0 mb-1 bg-card border border-border rounded-lg shadow-lg z-20 py-1 min-w-[140px]">
                  {CATEGORIES.map((cat) => (
                    <button
                      key={cat.value}
                      onClick={() => {
                        setCategory(cat.value);
                        setShowCategories(false);
                      }}
                      className={cn(
                        'w-full text-left px-3 py-1.5 text-xs transition-colors',
                        cat.value === category
                          ? 'bg-primary/10 text-primary font-medium'
                          : 'text-foreground hover:bg-secondary'
                      )}
                    >
                      {cat.label}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>

          {/* Text input + send */}
          <form onSubmit={handleSubmit} className="flex items-end gap-2">
            <textarea
              ref={inputRef}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Type your feedback..."
              rows={1}
              className="flex-1 resize-none rounded-xl border border-border bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/30 focus:border-primary/50 transition-colors"
              style={{ minHeight: '38px', maxHeight: '100px' }}
              onInput={(e) => {
                const target = e.target as HTMLTextAreaElement;
                target.style.height = '38px';
                target.style.height = Math.min(target.scrollHeight, 100) + 'px';
              }}
            />
            <button
              type="submit"
              disabled={!message.trim() || sending}
              className={cn(
                'w-9 h-9 rounded-xl flex items-center justify-center transition-all flex-shrink-0',
                message.trim() && !sending
                  ? 'bg-primary text-primary-foreground hover:opacity-90'
                  : 'bg-secondary text-muted-foreground cursor-not-allowed'
              )}
            >
              {sending ? (
                <div className="w-4 h-4 border-2 border-current/30 border-t-current rounded-full animate-spin" />
              ) : (
                <Send className="w-4 h-4" />
              )}
            </button>
          </form>
          <p className="text-[10px] text-muted-foreground/50 mt-1.5 text-center">
            Press Enter to send, Shift+Enter for new line
          </p>
        </div>
      </div>
    </>
  );
}
