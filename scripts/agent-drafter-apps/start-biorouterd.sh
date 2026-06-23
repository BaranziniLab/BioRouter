#!/usr/bin/env bash
# Start biorouterd for app testing using the CACHED provider key (no keychain
# prompt). Run from the worktree root.
cd /Users/wanjun/Desktop/biorouter-apps-wt
source bin/activate-hermit 2>/dev/null
export BIOROUTER_SERVER__SECRET_KEY=test
export BIOROUTER_ESBUILD_BIN=/Users/wanjun/Desktop/BioRouter/ui/desktop/node_modules/.bin/esbuild
export XIAOMI_MIMO_API_KEY=$(cat /tmp/br-mimo.key 2>/dev/null)
export XIAOMI_MIMO_HOST="https://token-plan-sgp.xiaomimimo.com/v1"
exec /tmp/br-apps-target/debug/biorouterd agent
