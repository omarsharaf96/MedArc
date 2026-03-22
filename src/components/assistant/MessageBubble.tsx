/**
 * MessageBubble.tsx — Chat message display for the AI Assistant.
 *
 * Renders user messages right-aligned (blue) and assistant messages left-aligned (gray).
 */

interface MessageBubbleProps {
  role: "user" | "assistant" | "system";
  content: string;
  timestamp?: string;
}

export function MessageBubble({ role, content, timestamp }: MessageBubbleProps) {
  if (role === "system") return null;

  const isUser = role === "user";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"} mb-3`}>
      <div
        className={[
          "max-w-[80%] rounded-lg px-4 py-2.5 text-sm leading-relaxed",
          isUser
            ? "bg-blue-600 text-white"
            : "bg-gray-100 text-gray-900 border border-gray-200",
        ].join(" ")}
      >
        <div className="whitespace-pre-wrap break-words">{content}</div>
        {timestamp && (
          <div
            className={`mt-1 text-xs ${isUser ? "text-blue-200" : "text-gray-400"}`}
          >
            {formatTime(timestamp)}
          </div>
        )}
      </div>
    </div>
  );
}

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}
