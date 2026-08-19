---
type: Metric
title: Monthly recurring revenue
description: Sum of active subscription value at month end.
br_page_id: 01J8XQ2M3N4P5R6S7T8V9W0XYZ
acme_cost_center: FIN-4412
acme_review:
  owner: "human:ahormati"
  cadence: quarterly
  checklist: [definition, filters, currency]
---

# Definition

OKF §4.1: producers "MAY include any additional keys"; consumers "SHOULD
preserve unknown keys when round-tripping and MUST NOT reject documents with
unrecognized fields". Losing `acme_review` on a read-modify-write is silent.
