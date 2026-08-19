---
type: BigQuery Table
title: Customer Orders
description: One row per completed customer order across all channels.
resource: https://console.cloud.google.com/bigquery?p=acme&d=sales&t=orders
tags: [sales, orders]
status: stable
stale_after: 2026-09-23
generated: { by: "reference_agent/gemini-2.5-pro", at: "2026-06-20T22:53:05Z" }
verified:
  - { by: "human:ahormati", at: "2026-06-25T09:00:00Z" }
  - { by: "process:finance-nightly", at: "2026-06-26T02:00:00Z" }
sources:
  - id: ga4-schema
    resource: https://developers.google.com/analytics/bigquery/export-schema
    title: GA4 BigQuery Export schema
    author: "team:ga4-docs"
    usage_count: 5000
    last_modified: 2026-05-30
  - id: orders-queries
    resource: all queries in BigQuery project acme
    usage_count: 120
usage_window: { from: 2026-06-01, to: 2026-06-30 }
---

# Schema

The `events_` table is sharded daily as `events_YYYYMMDD`.[^ga4-schema]

| Column        | Type      | Description                       |
|---------------|-----------|-----------------------------------|
| `order_id`    | STRING    | Globally unique order identifier. |
| `customer_id` | STRING    | Foreign key into customers.       |

# Joins

Joined with [customers](/tables/customers.md) on `customer_id`.

[^ga4-schema]: GA4 BigQuery Export schema
