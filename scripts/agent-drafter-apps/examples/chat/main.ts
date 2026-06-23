import { createApp } from "./sdk";

// Default chat app: createApp auto-mounts a chat panel into [data-br-chat],
// streams the agent's markdown reply, and handles the WebSocket to BioRouter.
createApp();
