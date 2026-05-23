'use client';

import { useState, useRef, useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { useMutation } from '@tanstack/react-query';
import {
  aiAssistantApi,
  AiMessage,
  BugReportDraftRequest,
  BugReportDraftResponse,
  BugReportPriority,
} from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import { toast } from 'sonner';
import { Sparkles, Send, ThumbsUp, ThumbsDown, Bug } from 'lucide-react';

export default function AiAssistantPage() {
  const router = useRouter();
  const hasModule = useAuthStore((s) => s.hasModule);
  const aiEnabled = hasModule('ai_assistant');

  const [mode, setMode] = useState<'chat' | 'bug_report'>('chat');
  const [messages, setMessages] = useState<AiMessage[]>([]);
  const [input, setInput] = useState('');
  const [conversationId, setConversationId] = useState<string | undefined>();
  const [feedback, setFeedback] = useState<Record<string, 'positive' | 'negative'>>({});
  const [bugNotes, setBugNotes] = useState('');
  const [bugDraft, setBugDraft] = useState<BugReportDraftResponse | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages]);

  // Redirect tenants that lack the ai_assistant module
  useEffect(() => {
    if (!aiEnabled) {
      router.replace('/dashboard');
    }
  }, [aiEnabled, router]);

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

  const bugDraftMutation = useMutation({
    mutationFn: (description: string) =>
      aiAssistantApi.generateBugReportDraft({ description }),
    onSuccess: (data) => {
      setBugDraft(data);
      setBugNotes('');
    },
    onError: () => {
      toast.error('Failed to generate bug report draft. Please try again.');
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed || mutation.isPending) return;
    mutation.mutate(trimmed);
  };

  const handleBugSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = bugNotes.trim();
    if (!trimmed || bugDraftMutation.isPending) return;
    bugDraftMutation.mutate(trimmed);
  };

  const handleFeedback = async (
    messageId: string,
    convId: string | undefined,
    rating: 'positive' | 'negative',
  ) => {
    if (!convId || feedback[messageId]) return;
    try {
      await aiAssistantApi.submitAnswerFeedback(convId, messageId, { rating });
      setFeedback((prev) => ({ ...prev, [messageId]: rating }));
    } catch {
      toast.error('Failed to submit feedback. Please try again.');
    }
  };

  const priorityColor: Record<BugReportPriority, string> = {
    low: 'bg-gray-100 text-gray-700',
    medium: 'bg-yellow-100 text-yellow-800',
    high: 'bg-orange-100 text-orange-800',
    critical: 'bg-red-100 text-red-800',
  };

  if (!aiEnabled) {
    return null;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b px-6 py-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            <h1 className="text-xl font-semibold">Winston AI Assistant</h1>
          </div>
          <div className="flex rounded-md border">
            <button
              type="button"
              onClick={() => setMode('chat')}
              className={`px-3 py-1.5 text-sm font-medium transition-colors ${
                mode === 'chat'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              } rounded-l-md`}
            >
              Chat
            </button>
            <button
              type="button"
              onClick={() => setMode('bug_report')}
              className={`px-3 py-1.5 text-sm font-medium transition-colors ${
                mode === 'bug_report'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              } rounded-r-md`}
            >
              <Bug className="inline h-3.5 w-3.5 mr-1" />
              Bug Report
            </button>
          </div>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          {mode === 'chat'
            ? 'Ask questions about invoices, vendors, or workflows.'
            : 'Describe a bug and get a structured report draft.'}
        </p>
      </div>

      {mode === 'chat' ? (
        <>
          <div ref={scrollRef} className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
            {messages.length === 0 && (
              <p className="text-center text-sm text-muted-foreground pt-12">
                Start a conversation by typing a message below.
              </p>
            )}
            {messages.map((msg) => (
              <div key={msg.id}>
                <div
                  className={`max-w-[80%] rounded-lg px-4 py-2 text-sm ${
                    msg.role === 'user'
                      ? 'ml-auto bg-primary text-primary-foreground'
                      : 'bg-muted'
                  }`}
                >
                  {msg.content}
                </div>
                {msg.role === 'assistant' && conversationId && (
                  <div className="mt-1 flex gap-1">
                    <button
                      type="button"
                      disabled={!!feedback[msg.id]}
                      onClick={() => handleFeedback(msg.id, conversationId, 'positive')}
                      className={`rounded p-1 text-xs transition-colors ${
                        feedback[msg.id] === 'positive'
                          ? 'text-green-600 bg-green-50'
                          : 'text-muted-foreground hover:text-green-600 hover:bg-green-50'
                      } disabled:opacity-60 disabled:cursor-default`}
                      aria-label="Thumbs up"
                    >
                      <ThumbsUp className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      disabled={!!feedback[msg.id]}
                      onClick={() => handleFeedback(msg.id, conversationId, 'negative')}
                      className={`rounded p-1 text-xs transition-colors ${
                        feedback[msg.id] === 'negative'
                          ? 'text-red-600 bg-red-50'
                          : 'text-muted-foreground hover:text-red-600 hover:bg-red-50'
                      } disabled:opacity-60 disabled:cursor-default`}
                      aria-label="Thumbs down"
                    >
                      <ThumbsDown className="h-3.5 w-3.5" />
                    </button>
                  </div>
                )}
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
        </>
      ) : (
        <>
          <div className="flex-1 overflow-y-auto px-6 py-4">
            {!bugDraft && !bugDraftMutation.isPending && (
              <p className="text-center text-sm text-muted-foreground pt-12">
                Describe the bug below and Winston will generate a structured report draft.
              </p>
            )}
            {bugDraftMutation.isPending && (
              <div className="text-center text-sm text-muted-foreground pt-12">
                Generating structured draft…
              </div>
            )}
            {bugDraft && (
              <div className="space-y-4 max-w-2xl mx-auto">
                <div>
                  <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Title</label>
                  <p className="mt-1 text-sm font-medium">{bugDraft.title}</p>
                </div>
                <div>
                  <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Current Behavior</label>
                  <p className="mt-1 text-sm">{bugDraft.current_behavior}</p>
                </div>
                <div>
                  <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Expected Behavior</label>
                  <p className="mt-1 text-sm">{bugDraft.expected_behavior}</p>
                </div>
                <div>
                  <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Reproduction Steps</label>
                  <ol className="mt-1 list-decimal list-inside space-y-1">
                    {bugDraft.reproduction_steps.map((step, i) => (
                      <li key={i} className="text-sm">{step}</li>
                    ))}
                  </ol>
                </div>
                <div className="flex gap-6">
                  <div>
                    <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Priority</label>
                    <span className={`ml-2 inline-block rounded px-2 py-0.5 text-xs font-medium ${priorityColor[bugDraft.priority]}`}>
                      {bugDraft.priority}
                    </span>
                  </div>
                  <div>
                    <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Affected Module</label>
                    <p className="mt-0.5 text-sm">{bugDraft.affected_module}</p>
                  </div>
                </div>
                <div>
                  <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Acceptance Criteria</label>
                  <ul className="mt-1 list-disc list-inside space-y-1">
                    {bugDraft.acceptance_criteria.map((c, i) => (
                      <li key={i} className="text-sm">{c}</li>
                    ))}
                  </ul>
                </div>
                <button
                  type="button"
                  onClick={() => setBugDraft(null)}
                  className="text-sm text-primary hover:underline"
                >
                  Generate another draft
                </button>
              </div>
            )}
          </div>

          <form onSubmit={handleBugSubmit} className="border-t px-6 py-4">
            <div className="flex gap-2">
              <textarea
                value={bugNotes}
                onChange={(e) => setBugNotes(e.target.value)}
                placeholder="Describe the bug: what happened, what you expected, and how to reproduce it…"
                rows={2}
                className="flex-1 resize-none rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                disabled={bugDraftMutation.isPending}
              />
              <button
                type="submit"
                disabled={bugDraftMutation.isPending || !bugNotes.trim()}
                className="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Send className="h-4 w-4" />
              </button>
            </div>
          </form>
        </>
      )}
    </div>
  );
}
