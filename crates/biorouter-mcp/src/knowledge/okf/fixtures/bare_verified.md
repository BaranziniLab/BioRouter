---
type: Playbook
title: "Incident response: data freshness alert"
description: Steps to triage a freshness alert on the orders pipeline.
verified: { by: "human:ahormati", at: "2026-06-25T09:00:00Z" }
---

# Trigger

OKF §5.2 permits `verified` as a bare mapping and §11 makes accepting it a
consumer MUST. A consumer that only understands the list form reads this page
as unverified — it does not error, it silently downgrades the page's trust.
