'use client';

import { useState, useRef, useEffect } from 'react';
import { useMutation } from '@tanstack/react-query';
import { aiAssistantApi, AiMessage } from '@/lib/api';
import { toast } from 'sonner';
import { Sparkles, Send } from 'lucide-react';

export default function AiAssistantPage() {
  const [messages, setMessages] = useState<AiMessage[]>([]);
  const [input, setInput] = useState('');
  const [conversationId, setConversationId] = useState<string | undefined>();
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages]);

  const mutation = useMutation({
    mutationFn: (message: string) =>
      aiAssistantApi.chat({ message, conversation_id: conversationId }),
    onSuccess: (data) => {
      const userMsg: AiMessage = {
        id: `local-${Date.now()}`,
        role: 'user',
        content: input,
        created_at: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, userMsg, data.message]);
      setConversationId(data.conversation_id);
      setInput('');
    },
    onError: () => {
      toast.error('Failed to get a response from Winston. Please try again.');
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed || mutation.isPending) return;
    mutation.mutate(trimmed);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="border-b px-6 py-4">
        <div className="flex items-center gap-2">
          <Sparkles className="h-5 w-5 text-primary" />
          <h1 className="text-xl font-semibold">Winston AI Assistant</h1>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          Ask questions about invoices, vendors, or workflows.
        </p>
      </div>

      <div ref={scrollRef} className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        {messages.length === 0 && (
          <p className="text-center text-sm text-muted-foreground pt-12">
            Start a conversation by typing a message below.
          </p>
        )}
        {messages.map((msg) => (
          <div
            key={msg.id}
            className={`max-w-[80%] rounded-lg px-4 py-2 text-sm ${
              msg.role === 'user'
                ? 'ml-auto bg-primary text-primary-foreground'
                : 'bg-muted'
            }`}
          >
            {msg.content}
          </div>
        ))}
        {mutation.isPending && (
          <div className="max-w-[80%] rounded-lg bg-muted px-4 py-2 text-sm text-muted-foreground">
            Thinking…
          </div>
        )}
      </div>

      <form onSubmit={handleSubmit} className="border-t px-6 py-4">
        <div className="flex gap-2">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Type your message…"
            rows={1}
            className="flex-1 resize-none rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                handleSubmit(e);
              }
            }}
            disabled={mutation.isPending}
          />
          <button
            type="submit"
            disabled={mutation.isPending || !input.trim()}
            className="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Send className="h-4 w-4" />
          </button>
        </div>
      </form>
    </div>
  );
}
