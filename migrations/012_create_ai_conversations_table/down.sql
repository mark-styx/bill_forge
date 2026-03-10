-- Drop AI tables
DROP TRIGGER IF EXISTS ai_message_created ON ai_messages;
DROP FUNCTION IF EXISTS update_conversation_timestamp();
DROP TABLE IF EXISTS ai_messages;
DROP TABLE IF EXISTS ai_conversations;
