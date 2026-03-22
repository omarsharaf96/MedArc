/**
 * AssistantPanel.tsx — Slide-out AI assistant chat panel.
 *
 * Accessible via a floating button. Contains:
 * - Conversation list (sidebar toggle)
 * - Active chat with message bubbles
 * - Action confirmation cards
 * - Text input + send button
 */

import { useState, useEffect, useRef, useCallback } from "react";
import { commands } from "../../lib/tauri";
import type {
  ConversationSummary,
  ConversationMessage,
  AssistantAction,
  ActionResult,
} from "../../types/assistant";
import { MessageBubble } from "./MessageBubble";
import { ActionCard } from "./ActionCard";
import { useDictation } from "../../hooks/useDictation";

// ─── Props ──────────────────────────────────────────────────────────────────

interface AssistantPanelProps {
  open: boolean;
  onClose: () => void;
}

// ─── Types for local state ──────────────────────────────────────────────────

interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  actions?: AssistantAction[] | null;
  createdAt?: string;
}

// ─── Component ──────────────────────────────────────────────────────────────

export function AssistantPanel({ open, onClose }: AssistantPanelProps) {
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [activeConversationId, setActiveConversationId] = useState<string | null>(null);
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [inputText, setInputText] = useState("");
  const [loading, setLoading] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Dictation — voice input
  const dictation = useDictation({
    onTranscript: (text) => {
      setInputText((prev) => (prev ? prev + " " + text : text));
      inputRef.current?.focus();
    },
  });

  // Scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Focus input when panel opens
  useEffect(() => {
    if (open) {
      setTimeout(() => inputRef.current?.focus(), 100);
    }
  }, [open]);

  // Load conversations on open
  useEffect(() => {
    if (open) {
      loadConversations();
    }
  }, [open]);

  async function loadConversations() {
    try {
      const convs = await commands.listConversations();
      setConversations(convs);
    } catch {
      // Silently fail — conversations list is not critical
    }
  }

  async function loadConversation(convId: string) {
    try {
      const msgs = await commands.getConversation(convId);
      setActiveConversationId(convId);
      setMessages(
        msgs.map((m: ConversationMessage) => ({
          id: m.id,
          role: m.role,
          content: m.content,
          actions: m.actionsJson ? JSON.parse(m.actionsJson) : null,
          createdAt: m.createdAt,
        }))
      );
      setShowHistory(false);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }

  function startNewConversation() {
    setActiveConversationId(null);
    setMessages([]);
    setShowHistory(false);
    setError(null);
  }

  const handleSend = useCallback(async () => {
    const text = inputText.trim();
    if (!text || loading) return;

    setInputText("");
    setError(null);

    // Optimistically add user message
    const tempId = `temp-${Date.now()}`;
    setMessages((prev) => [
      ...prev,
      { id: tempId, role: "user", content: text },
    ]);

    setLoading(true);
    try {
      const response = await commands.sendAssistantMessage({
        message: text,
        conversationId: activeConversationId,
      });

      // Update conversation ID (may be new)
      setActiveConversationId(response.conversationId);

      // Add assistant response
      setMessages((prev) => [
        ...prev,
        {
          id: response.messageId,
          role: "assistant",
          content: response.content,
          actions: response.actions,
        },
      ]);

      // Refresh conversations list
      loadConversations();
    } catch (err) {
      setError(String(err));
      // Add error as assistant message
      setMessages((prev) => [
        ...prev,
        {
          id: `error-${Date.now()}`,
          role: "assistant",
          content: `Sorry, I encountered an error: ${String(err)}`,
        },
      ]);
    } finally {
      setLoading(false);
    }
  }, [inputText, loading, activeConversationId]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function handleActionComplete(_result: ActionResult) {
    // Result is shown inline by ActionCard
    loadConversations();
  }

  async function handleDeleteConversation(convId: string) {
    try {
      await commands.deleteConversation(convId);
      if (activeConversationId === convId) {
        startNewConversation();
      }
      loadConversations();
    } catch (err) {
      setError(String(err));
    }
  }

  if (!open) return null;

  return (
    <>
      {/* Panel — no backdrop so the user can interact with the main content while the panel is open */}
      <div className="fixed right-0 top-0 z-50 flex h-full w-[420px] max-w-full flex-col border-l border-gray-200 bg-white shadow-xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-gray-200 px-4 py-3">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setShowHistory(!showHistory)}
              className="rounded p-1 text-gray-500 hover:bg-gray-100 hover:text-gray-700"
              title="Conversation history"
            >
              <svg
                className="h-5 w-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
            </button>
            <h2 className="text-sm font-semibold text-gray-900">
              AI Assistant
            </h2>
          </div>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={startNewConversation}
              className="rounded p-1 text-gray-500 hover:bg-gray-100 hover:text-gray-700"
              title="New conversation"
            >
              <svg
                className="h-5 w-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 4v16m8-8H4"
                />
              </svg>
            </button>
            <button
              type="button"
              onClick={onClose}
              className="rounded p-1 text-gray-500 hover:bg-gray-100 hover:text-gray-700"
              title="Close"
            >
              <svg
                className="h-5 w-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </button>
          </div>
        </div>

        {/* Conversation history dropdown */}
        {showHistory && (
          <div className="border-b border-gray-200 bg-gray-50 max-h-64 overflow-y-auto">
            <div className="px-3 py-2">
              <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
                Recent Conversations
              </div>
              {conversations.length === 0 ? (
                <p className="py-2 text-xs text-gray-400">
                  No conversations yet.
                </p>
              ) : (
                <ul className="space-y-1">
                  {conversations.map((conv) => (
                    <li key={conv.id} className="flex items-center gap-1">
                      <button
                        type="button"
                        onClick={() => loadConversation(conv.id)}
                        className={`flex-1 rounded px-2 py-1.5 text-left text-xs transition-colors ${
                          activeConversationId === conv.id
                            ? "bg-blue-100 text-blue-700"
                            : "text-gray-700 hover:bg-gray-100"
                        }`}
                      >
                        <div className="truncate font-medium">
                          {conv.title || "Untitled"}
                        </div>
                        <div className="truncate text-gray-400">
                          {formatDate(conv.updatedAt)}
                        </div>
                      </button>
                      <button
                        type="button"
                        onClick={() => handleDeleteConversation(conv.id)}
                        className="rounded p-1 text-gray-400 hover:bg-red-50 hover:text-red-500"
                        title="Delete"
                      >
                        <svg
                          className="h-3.5 w-3.5"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            strokeWidth={2}
                            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                          />
                        </svg>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        )}

        {/* Messages area */}
        <div className="flex-1 overflow-y-auto px-4 py-4">
          {messages.length === 0 && !loading && (
            <div className="flex flex-col items-center justify-center h-full text-center">
              <div className="mb-3 text-3xl">
                <svg className="h-12 w-12 text-blue-200 mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
                </svg>
              </div>
              <p className="text-sm font-medium text-gray-700">
                PanaceaEMR Assistant
              </p>
              <p className="mt-1 text-xs text-gray-400 max-w-[280px]">
                Ask me to schedule appointments, look up patients, check your
                schedule, or find patients who need follow-up.
              </p>
              <div className="mt-4 space-y-1.5">
                {[
                  "What appointments do I have today?",
                  "Schedule Jane Doe for PT treatment tomorrow at 2pm",
                  "Show me patients not seen in 30 days",
                  "Write a progress note for Jane Doe",
                  "Export patient chart for John Smith",
                ].map((suggestion) => (
                  <button
                    key={suggestion}
                    type="button"
                    onClick={() => {
                      setInputText(suggestion);
                      inputRef.current?.focus();
                    }}
                    className="block w-full rounded-md border border-gray-200 bg-gray-50 px-3 py-2 text-left text-xs text-gray-600 transition-colors hover:border-blue-300 hover:bg-blue-50"
                  >
                    {suggestion}
                  </button>
                ))}
              </div>
            </div>
          )}

          {messages.map((msg) => (
            <div key={msg.id}>
              <MessageBubble
                role={msg.role}
                content={msg.content}
                timestamp={msg.createdAt}
              />
              {msg.actions &&
                msg.actions.length > 0 &&
                activeConversationId && (
                  <ActionCard
                    actions={msg.actions}
                    conversationId={activeConversationId}
                    messageId={msg.id}
                    onActionComplete={handleActionComplete}
                  />
                )}
            </div>
          ))}

          {loading && (
            <div className="flex justify-start mb-3">
              <div className="rounded-lg bg-gray-100 border border-gray-200 px-4 py-2.5">
                <div className="flex items-center gap-1.5">
                  <div className="h-2 w-2 animate-bounce rounded-full bg-gray-400" style={{ animationDelay: "0ms" }} />
                  <div className="h-2 w-2 animate-bounce rounded-full bg-gray-400" style={{ animationDelay: "150ms" }} />
                  <div className="h-2 w-2 animate-bounce rounded-full bg-gray-400" style={{ animationDelay: "300ms" }} />
                </div>
              </div>
            </div>
          )}

          {error && (
            <div className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-xs text-red-600">
              {error}
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>

        {/* Input area */}
        <div className="border-t border-gray-200 bg-white px-4 py-3">
          {/* Recording / transcribing status bar */}
          {(dictation.isRecording || dictation.isTranscribing) && (
            <div className="mb-2 flex items-center gap-2 rounded-lg bg-red-50 border border-red-200 px-3 py-2">
              {dictation.isRecording ? (
                <>
                  <span className="relative flex h-3 w-3">
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-400 opacity-75" />
                    <span className="relative inline-flex h-3 w-3 rounded-full bg-red-500" />
                  </span>
                  <span className="text-xs font-medium text-red-700 flex-1">
                    Recording... {dictation.elapsed}s
                  </span>
                  {/* Audio level bar */}
                  <div className="w-16 h-2 bg-red-100 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-red-500 rounded-full transition-all duration-100"
                      style={{ width: `${Math.min(dictation.audioLevel * 100, 100)}%` }}
                    />
                  </div>
                  <button
                    type="button"
                    onClick={() => void dictation.stop()}
                    className="rounded-md bg-red-600 px-2 py-1 text-xs font-medium text-white hover:bg-red-700"
                  >
                    Stop
                  </button>
                </>
              ) : (
                <>
                  <svg className="h-4 w-4 animate-spin text-blue-500" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                  </svg>
                  <span className="text-xs font-medium text-blue-700">Transcribing...</span>
                </>
              )}
            </div>
          )}
          {dictation.error && (
            <p className="mb-2 text-xs text-red-600">{dictation.error}</p>
          )}
          <div className="flex items-end gap-2">
            {/* Microphone / dictation button */}
            <button
              type="button"
              onClick={() => void dictation.toggle()}
              disabled={loading || dictation.isTranscribing}
              title={dictation.isRecording ? "Stop recording" : "Speak (voice input)"}
              className={`rounded-lg p-2 shadow-sm transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                dictation.isRecording
                  ? "bg-red-600 text-white hover:bg-red-700 animate-pulse"
                  : "bg-gray-100 text-gray-600 hover:bg-gray-200"
              }`}
            >
              <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4M12 15a3 3 0 003-3V5a3 3 0 00-6 0v7a3 3 0 003 3z" />
              </svg>
            </button>
            <textarea
              ref={inputRef}
              value={inputText}
              onChange={(e) => setInputText(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={dictation.isRecording ? "Listening..." : "Type or speak..."}
              rows={1}
              disabled={dictation.isRecording}
              className="flex-1 resize-none rounded-lg border border-gray-300 bg-gray-50 px-3 py-2 text-sm text-gray-900 placeholder-gray-400 focus:border-blue-500 focus:bg-white focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:opacity-60"
              style={{ maxHeight: "120px" }}
              onInput={(e) => {
                const target = e.target as HTMLTextAreaElement;
                target.style.height = "auto";
                target.style.height = `${Math.min(target.scrollHeight, 120)}px`;
              }}
            />
            <button
              type="button"
              onClick={handleSend}
              disabled={!inputText.trim() || loading}
              className="rounded-lg bg-blue-600 p-2 text-white shadow-sm transition-colors hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
              title="Send message"
            >
              <svg
                className="h-5 w-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
                />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    const days = Math.floor(diff / (1000 * 60 * 60 * 24));

    if (days === 0) return "Today";
    if (days === 1) return "Yesterday";
    if (days < 7) return `${days} days ago`;
    return d.toLocaleDateString();
  } catch {
    return "";
  }
}
