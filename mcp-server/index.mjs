#!/usr/bin/env node
/**
 * Raydrop MCP Stdio Server (Zero external dependencies)
 * Bridges standard MCP JSON-RPC 2.0 (stdin/stdout) to the running Raydrop Desktop app
 * at http://127.0.0.1:45321/mcp (override via RAYDROP_MCP_URL).
 */

import readline from 'node:readline';

const MCP_URL = process.env.RAYDROP_MCP_URL || 'http://127.0.0.1:45321/mcp';

const rl = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

function writeResponse(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

rl.on('line', async (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;

  let req;
  try {
    req = JSON.parse(trimmed);
  } catch (err) {
    writeResponse({
      jsonrpc: '2.0',
      id: null,
      error: { code: -32700, message: `Parse error: ${err.message}` },
    });
    return;
  }

  // Notifications without an id do not expect a response in JSON-RPC 2.0
  const isNotification = req.id === undefined || req.id === null;

  try {
    const res = await fetch(MCP_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });

    if (isNotification) return;

    const text = await res.text();
    if (!text) {
      writeResponse({ jsonrpc: '2.0', id: req.id, result: {} });
      return;
    }

    const json = JSON.parse(text);
    writeResponse(json);
  } catch (err) {
    if (isNotification) return;
    writeResponse({
      jsonrpc: '2.0',
      id: req.id,
      error: {
        code: -32000,
        message: `Raydrop desktop app is not reachable at ${MCP_URL}. Please open Raydrop first. (${err.message})`,
      },
    });
  }
});
