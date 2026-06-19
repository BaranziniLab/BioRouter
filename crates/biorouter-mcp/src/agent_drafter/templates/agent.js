/* Agent Drafter — embedded agent runtime.
 *
 * Ships inside every agent-enabled artifact. It gives the artifact a uniform
 * `window.BioRouterAgent` API regardless of where it runs:
 *
 *   - transport "acp-ws":  the artifact is standalone (e.g. a Tauri export or a
 *       web page). It speaks the Agent Client Protocol (ACP) as a JSON-RPC client
 *       over a WebSocket to a BioRouter agent (a `biorouter acp` sidecar bridged
 *       to a socket, or a hosted biorouterd). This is the full conversational mode.
 *
 *   - transport "bridge":  the artifact is previewed *inside* BioRouter in the
 *       sandboxed MCP-App iframe. It talks to the host over postMessage JSON-RPC
 *       (ui/message, tools/call). Full streaming back into the iframe requires the
 *       host bridge extension; until then prompts are routed into the BioRouter
 *       chat and the artifact is notified.
 *
 * Configure via `window.BIOROUTER_AGENT_CONFIG = { transport, endpoint,
 * systemPrompt, greeting, tools }` (Agent Drafter injects this for you).
 */
(function () {
  "use strict";
  var cfg = window.BIOROUTER_AGENT_CONFIG || {};
  var transport = cfg.transport || (cfg.endpoint ? "acp-ws" : "bridge");
  var listeners = { ready: [], message: [], thought: [], tool: [], error: [] };

  function emit(kind, payload) {
    (listeners[kind] || []).forEach(function (fn) {
      try { fn(payload); } catch (e) { /* listener errors are non-fatal */ }
    });
  }

  // --- ACP-over-WebSocket transport (standalone / full conversational) --------
  var ws = null, wsReady = null, rpcId = 0, pending = {}, sessionId = null;

  function acpConnect() {
    if (wsReady) return wsReady;
    wsReady = new Promise(function (resolve, reject) {
      try { ws = new WebSocket(cfg.endpoint); }
      catch (e) { reject(e); return; }
      ws.onopen = function () {
        acpRequest("initialize", { protocolVersion: 1 })
          .then(function () { return acpRequest("session/new", { cwd: cfg.cwd || ".", mcpServers: [] }); })
          .then(function (res) { sessionId = (res && res.sessionId) || (res && res.session_id); emit("ready", {}); resolve(); })
          .catch(reject);
      };
      ws.onerror = function (e) { emit("error", e); reject(e); };
      ws.onmessage = function (ev) {
        var msg; try { msg = JSON.parse(ev.data); } catch (e) { return; }
        if (msg.id != null && pending[msg.id]) {
          var p = pending[msg.id]; delete pending[msg.id];
          if (msg.error) p.reject(msg.error); else p.resolve(msg.result);
        } else if (msg.method === "session/update") {
          handleSessionUpdate(msg.params && msg.params.update);
        }
      };
    });
    return wsReady;
  }

  function acpRequest(method, params) {
    return new Promise(function (resolve, reject) {
      var id = ++rpcId;
      pending[id] = { resolve: resolve, reject: reject };
      ws.send(JSON.stringify({ jsonrpc: "2.0", id: id, method: method, params: params || {} }));
    });
  }

  function handleSessionUpdate(u) {
    if (!u) return;
    var kind = u.sessionUpdate || u.session_update;
    var text = (u.content && (u.content.text != null ? u.content.text : (u.content.content && u.content.content.text)));
    if (kind === "agent_message_chunk") emit("message", { delta: text || "" });
    else if (kind === "agent_thought_chunk") emit("thought", { delta: text || "" });
    else if (kind === "tool_call" || kind === "tool_call_update") emit("tool", u);
  }

  function acpPrompt(text) {
    return acpConnect().then(function () {
      return acpRequest("session/prompt", {
        sessionId: sessionId,
        prompt: [{ type: "text", text: text }]
      });
    });
  }

  // --- MCP-App bridge transport (in-BioRouter preview) ------------------------
  var bridgePending = {};
  if (transport === "bridge") {
    window.addEventListener("message", function (ev) {
      var msg = ev.data;
      if (!msg || msg.jsonrpc !== "2.0") return;
      if (msg.id != null && bridgePending[msg.id]) {
        var p = bridgePending[msg.id]; delete bridgePending[msg.id];
        if (msg.error) p.reject(msg.error); else p.resolve(msg.result);
      }
    });
    setTimeout(function () { emit("ready", {}); }, 0);
  }

  function bridgeRequest(method, params) {
    return new Promise(function (resolve, reject) {
      var id = "br-" + (++rpcId);
      bridgePending[id] = { resolve: resolve, reject: reject };
      window.parent.postMessage({ jsonrpc: "2.0", id: id, method: method, params: params || {} }, "*");
    });
  }

  function bridgePrompt(text) {
    // Route into the BioRouter conversation; full in-iframe streaming is a
    // host-bridge capability tracked separately.
    return bridgeRequest("ui/message", { type: "append", text: text }).then(function () {
      emit("message", { delta: "", note: "Sent to BioRouter. The agent's reply appears in the chat." });
      return { routed: true };
    });
  }

  // --- Public API -------------------------------------------------------------
  var Agent = {
    config: cfg,
    transport: transport,
    on: function (kind, fn) { if (listeners[kind]) listeners[kind].push(fn); return Agent; },
    connect: function () { return transport === "acp-ws" ? acpConnect() : Promise.resolve(); },
    prompt: function (text) { return transport === "acp-ws" ? acpPrompt(text) : bridgePrompt(text); },
    callTool: function (name, args) {
      if (transport === "bridge") return bridgeRequest("tools/call", { name: name, arguments: args || {} });
      return acpConnect().then(function () { return acpRequest("session/prompt", { sessionId: sessionId, prompt: [{ type: "text", text: "Use tool " + name }] }); });
    }
  };
  window.BioRouterAgent = Agent;

  // --- Optional auto-mounted chat panel ---------------------------------------
  function mountChat() {
    var host = document.querySelector("[data-br-chat]");
    if (!host) return;
    host.classList.add("br-chat");
    var log = document.createElement("div"); log.className = "br-chat__log";
    var form = document.createElement("form"); form.className = "br-chat__form";
    var input = document.createElement("input"); input.className = "br-input"; input.placeholder = "Ask the agent…";
    var send = document.createElement("button"); send.className = "br-btn"; send.type = "submit"; send.textContent = "Send";
    form.appendChild(input); form.appendChild(send);
    host.appendChild(log); host.appendChild(form);

    function add(cls, text) {
      var el = document.createElement("div"); el.className = "br-msg br-msg--" + cls; el.textContent = text;
      log.appendChild(el); log.scrollTop = log.scrollHeight; return el;
    }
    if (cfg.greeting) add("agent", cfg.greeting);

    var current = null;
    Agent.on("message", function (m) { if (!current) current = add("agent", ""); current.textContent += (m.delta || m.note || ""); log.scrollTop = log.scrollHeight; });
    Agent.on("thought", function (m) { add("thought", m.delta || ""); });
    Agent.on("tool", function (u) { add("tool", "⚙ " + (u.title || u.toolCallId || "tool")); });
    Agent.on("error", function () { add("agent", "⚠ Could not reach the agent."); });

    form.addEventListener("submit", function (e) {
      e.preventDefault();
      var text = input.value.trim(); if (!text) return;
      add("user", text); input.value = ""; current = null;
      Agent.prompt(text).catch(function () { add("agent", "⚠ Failed to send."); });
    });
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", mountChat);
  else mountChat();
})();
