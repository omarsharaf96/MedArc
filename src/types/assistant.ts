/**
 * TypeScript types for the AI Assistant (Phase 1).
 *
 * Field names use camelCase to match the Rust structs'
 * #[serde(rename_all = "camelCase")].
 */

/** Input for sending a message to the assistant. */
export interface SendMessageInput {
  message: string;
  conversationId?: string | null;
}

/** A parsed action from the assistant's response. */
export interface AssistantAction {
  action: string;
  [key: string]: unknown;
}

/** Response from the assistant. */
export interface AssistantResponse {
  conversationId: string;
  messageId: string;
  content: string;
  actions: AssistantAction[] | null;
  modelUsed: string;
}

/** Summary of a conversation. */
export interface ConversationSummary {
  id: string;
  title: string | null;
  createdAt: string;
  updatedAt: string;
  lastMessage: string | null;
}

/** A single message in a conversation. */
export interface ConversationMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  actionsJson: string | null;
  createdAt: string;
}

/** Input for executing an assistant action. */
export interface ExecuteActionInput {
  action: string;
  params: Record<string, unknown>;
  conversationId: string;
}

/** Result of an action execution. */
export interface ActionResult {
  success: boolean;
  message: string;
  data: Record<string, unknown> | null;
}
