//! `biorouter usage` — per-day / per-model token + cost usage reports and a
//! month-to-date summary against the configured (display-only) monthly budget.
//!
//! Cost is computed through the same shared
//! [`biorouter::providers::pricing::model_cost`] helper the server and GUI use,
//! so the CLI can never disagree with the app on what a turn cost. Unknown
//! models show a `—` cost, never a misleading `$0`.

use anyhow::Result;
use biorouter::config::{percent_of, UsageLimits};
use biorouter::session::{SessionManager, UsageGroup, UsageReportRow, UsageSummary};
use chrono::{Local, NaiveDate, TimeZone};

/// Parse a `YYYY-MM-DD` date to the unix second of its local `start`-of-day
/// (`false`) or last second of the day (`true`).
fn parse_local_date(date: &str, end_of_day: bool) -> Result<i64> {
    let day = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("invalid date '{date}', expected YYYY-MM-DD"))?;
    let time = if end_of_day {
        day.and_hms_opt(23, 59, 59).unwrap()
    } else {
        day.and_hms_opt(0, 0, 0).unwrap()
    };
    // A skipped/ambiguous local wall-clock instant (DST) resolves to the
    // earliest valid one; good enough for a day-granularity report boundary.
    Ok(Local
        .from_local_datetime(&time)
        .earliest()
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| Local.from_utc_datetime(&time).timestamp()))
}

pub async fn handle_usage(
    from: Option<String>,
    to: Option<String>,
    by_model: bool,
    json: bool,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let to_ts = match to {
        Some(d) => parse_local_date(&d, true)?,
        None => now,
    };
    let from_ts = match from {
        Some(d) => parse_local_date(&d, false)?,
        None => to_ts - 30 * 86_400,
    };
    let group = if by_model {
        UsageGroup::Model
    } else {
        UsageGroup::Day
    };

    let manager = SessionManager::instance();
    let rows = manager.get_usage_report(from_ts, to_ts, group).await?;
    let summary = manager.get_usage_summary().await?;
    let limits = UsageLimits::load();

    if json {
        println!("{}", usage_json(&rows, group, &summary, &limits)?);
    } else {
        print!(
            "{}",
            format_usage_text(&rows, group, &summary, &limits, from_ts, to_ts)
        );
    }
    Ok(())
}

/// Machine-readable output: the report rows plus the summary and configured
/// limits with their percents, mirroring the two HTTP routes.
fn usage_json(
    rows: &[UsageReportRow],
    group: UsageGroup,
    summary: &UsageSummary,
    limits: &UsageLimits,
) -> Result<String> {
    let token_percent = percent_of(
        summary.month_to_date.total_tokens as f64,
        limits.monthly_token_limit.map(|l| l as f64),
    );
    let dollar_percent = summary
        .month_to_date
        .cost
        .and_then(|c| percent_of(c, limits.monthly_dollar_limit));

    let value = serde_json::json!({
        "report": {
            "group": group,
            "rows": rows,
        },
        "summary": summary,
        "monthlyTokenLimit": limits.monthly_token_limit,
        "monthlyDollarLimit": limits.monthly_dollar_limit,
        "tokenPercent": token_percent,
        "dollarPercent": dollar_percent,
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

fn fmt_tokens(n: i64) -> String {
    // Thousands separators, e.g. 13_300_000 -> "13,300,000".
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn fmt_cost(cost: Option<f64>) -> String {
    match cost {
        // Sub-cent but non-zero: show enough precision to not read as $0.00.
        Some(c) if c > 0.0 && c < 0.01 => format!("${c:.4}"),
        Some(c) => format!("${c:.2}"),
        None => "—".to_string(),
    }
}

fn model_label(row: &UsageReportRow) -> String {
    match (row.model_id.as_deref(), row.provider.as_deref()) {
        (Some(model), Some(provider)) => format!("{model} ({provider})"),
        (Some(model), None) => model.to_string(),
        _ => "unknown".to_string(),
    }
}

/// Human-readable report + summary. Pure so the formatting is unit-tested
/// without a database.
fn format_usage_text(
    rows: &[UsageReportRow],
    group: UsageGroup,
    summary: &UsageSummary,
    limits: &UsageLimits,
    from_ts: i64,
    to_ts: i64,
) -> String {
    let mut out = String::new();
    let from_day = Local
        .timestamp_opt(from_ts, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let to_day = Local
        .timestamp_opt(to_ts, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();

    let heading = match group {
        UsageGroup::Model => "Usage by model",
        _ => "Usage by day",
    };
    out.push_str(&format!("{heading}  ({from_day} → {to_day})\n"));

    if rows.is_empty() {
        out.push_str("  (no usage in this range)\n");
    } else {
        let label_header = if matches!(group, UsageGroup::Model) {
            "MODEL"
        } else {
            "DAY"
        };
        out.push_str(&format!(
            "  {label_header:<28} {:>14} {:>14} {:>14} {:>14} {:>7} {:>12}\n",
            "INPUT", "OUTPUT", "CACHE", "TOTAL", "TURNS", "COST"
        ));
        let mut any_unpriced = false;
        let mut any_cost_excludes_cache = false;
        for row in rows {
            let label = match group {
                UsageGroup::Model => model_label(row),
                _ => row.date.clone().unwrap_or_else(|| "unknown".to_string()),
            };
            any_unpriced |= row.has_unpriced;
            any_cost_excludes_cache |= row.cost_excludes_cache;
            out.push_str(&format!(
                "  {label:<28} {:>14} {:>14} {:>14} {:>14} {:>7} {:>12}\n",
                fmt_tokens(row.input_tokens),
                fmt_tokens(row.output_tokens),
                fmt_tokens(row.cache_read_tokens + row.cache_creation_tokens),
                fmt_tokens(row.total_tokens),
                row.turns,
                fmt_cost(row.cost),
            ));
        }
        if any_unpriced {
            out.push_str(
                "  Note: some rows include models with no known pricing; their cost is excluded (—).\n",
            );
        }
        if any_cost_excludes_cache {
            out.push_str(
                "  Note: some priced models have no cache pricing; their cache tokens are excluded from cost.\n",
            );
        }
    }

    // Month-to-date summary vs the configured budget.
    out.push('\n');
    out.push_str(&format!("Month to date ({})\n", summary.month));
    let mtd = &summary.month_to_date;
    out.push_str(&format!(
        "  Tokens: {}   Cost: {}   Turns: {}\n",
        fmt_tokens(mtd.total_tokens),
        fmt_cost(mtd.cost),
        mtd.turns,
    ));
    let mtd_cache = mtd.cache_read_tokens + mtd.cache_creation_tokens;
    if mtd_cache > 0 {
        out.push_str(&format!(
            "  Cache: {} read, {} write\n",
            fmt_tokens(mtd.cache_read_tokens),
            fmt_tokens(mtd.cache_creation_tokens),
        ));
    }
    if let Some(limit) = limits.monthly_token_limit {
        let pct = percent_of(mtd.total_tokens as f64, Some(limit as f64)).unwrap_or(0.0);
        out.push_str(&format!(
            "  Token budget: {} / {}  ({pct:.1}%)\n",
            fmt_tokens(mtd.total_tokens),
            fmt_tokens(limit),
        ));
    }
    if let Some(limit) = limits.monthly_dollar_limit {
        match mtd.cost.and_then(|c| percent_of(c, Some(limit))) {
            Some(pct) => out.push_str(&format!(
                "  Dollar budget: {} / ${:.2}  ({pct:.1}%)\n",
                fmt_cost(mtd.cost),
                limit,
            )),
            None => out.push_str(&format!(
                "  Dollar budget: — / ${limit:.2}  (cost unavailable — some models unpriced)\n"
            )),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::session::UsageTotals;

    fn row(
        label: &str,
        is_model: bool,
        input: i64,
        output: i64,
        total: i64,
        turns: i64,
        cost: Option<f64>,
        has_unpriced: bool,
    ) -> UsageReportRow {
        UsageReportRow {
            date: (!is_model).then(|| label.to_string()),
            model_id: is_model.then(|| label.to_string()),
            provider: is_model.then(|| "zai".to_string()),
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            turns,
            cost,
            has_unpriced,
            cost_excludes_cache: false,
        }
    }

    fn summary(total: i64, cost: Option<f64>) -> UsageSummary {
        let totals = || UsageTotals {
            input_tokens: total,
            output_tokens: 0,
            total_tokens: total,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            turns: 3,
            cost,
            has_unpriced: cost.is_none(),
            cost_excludes_cache: false,
        };
        UsageSummary {
            month: "2026-07".to_string(),
            month_to_date: totals(),
            all_time: totals(),
        }
    }

    #[test]
    fn tokens_get_thousands_separators() {
        assert_eq!(fmt_tokens(13_300_000), "13,300,000");
        assert_eq!(fmt_tokens(999), "999");
        assert_eq!(fmt_tokens(1_000), "1,000");
        assert_eq!(fmt_tokens(0), "0");
    }

    #[test]
    fn cost_formatting_never_reads_zero_for_unknown() {
        assert_eq!(fmt_cost(None), "—");
        assert_eq!(fmt_cost(Some(5.0)), "$5.00");
        assert_eq!(fmt_cost(Some(1234.5)), "$1234.50");
        // Sub-cent shows extra precision rather than $0.00.
        assert_eq!(fmt_cost(Some(0.0034)), "$0.0034");
    }

    #[test]
    fn text_report_by_day_shows_rows_and_budget() {
        let rows = vec![
            row(
                "2026-07-10",
                false,
                1_000_000,
                0,
                1_000_000,
                5,
                Some(1.40),
                true,
            ),
            row(
                "2026-07-11",
                false,
                2_000_000,
                1_000_000,
                3_000_000,
                3,
                Some(7.20),
                false,
            ),
        ];
        let limits = UsageLimits {
            monthly_token_limit: Some(66_000_000),
            monthly_dollar_limit: Some(250.0),
        };
        let text = format_usage_text(
            &rows,
            UsageGroup::Day,
            &summary(33_000_000, Some(8.60)),
            &limits,
            0,
            0,
        );
        assert!(text.contains("Usage by day"), "{text}");
        assert!(text.contains("2026-07-10"));
        assert!(text.contains("1,000,000"));
        assert!(text.contains("$1.40"));
        assert!(text.contains("$7.20"));
        // MTD budget: 33M / 66M = 50%.
        assert!(text.contains("50.0%"), "{text}");
        assert!(text.contains("Month to date (2026-07)"));
        // At least one row is partially unpriced → the note appears.
        assert!(text.contains("no known pricing"), "{text}");
    }

    #[test]
    fn text_report_by_model_flags_unknown_cost() {
        let rows = vec![
            row(
                "glm-5.2",
                true,
                3_000_000,
                1_000_000,
                4_000_000,
                8,
                Some(8.60),
                false,
            ),
            // The unknown bucket: null cost, must render as —.
            UsageReportRow {
                date: None,
                model_id: None,
                provider: None,
                input_tokens: 500,
                output_tokens: 500,
                total_tokens: 1_000,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                turns: 2,
                cost: None,
                has_unpriced: true,
                cost_excludes_cache: false,
            },
        ];
        let text = format_usage_text(
            &rows,
            UsageGroup::Model,
            &summary(4_001_000, None),
            &UsageLimits::default(),
            0,
            0,
        );
        assert!(text.contains("Usage by model"), "{text}");
        assert!(text.contains("glm-5.2 (zai)"));
        assert!(text.contains("unknown"));
        assert!(text.contains("—"), "null cost shows an em dash: {text}");
        // No configured limits → no budget lines.
        assert!(!text.contains("Token budget"));
        assert!(!text.contains("Dollar budget"));
    }

    #[test]
    fn dollar_budget_reports_unavailable_when_cost_unknown() {
        let limits = UsageLimits {
            monthly_token_limit: None,
            monthly_dollar_limit: Some(100.0),
        };
        let text = format_usage_text(&[], UsageGroup::Day, &summary(500, None), &limits, 0, 0);
        assert!(text.contains("cost unavailable"), "{text}");
        assert!(text.contains("(no usage in this range)"));
    }

    #[test]
    fn json_output_carries_report_and_percents() {
        let rows = vec![row(
            "2026-07-10",
            false,
            1_000_000,
            0,
            1_000_000,
            5,
            Some(1.40),
            false,
        )];
        let limits = UsageLimits {
            monthly_token_limit: Some(66_000_000),
            monthly_dollar_limit: Some(250.0),
        };
        let json = usage_json(
            &rows,
            UsageGroup::Day,
            &summary(33_000_000, Some(125.0)),
            &limits,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["report"]["group"], "day");
        assert_eq!(value["report"]["rows"][0]["totalTokens"], 1_000_000);
        assert_eq!(value["monthlyTokenLimit"], 66_000_000);
        // 33M / 66M = 50%, $125 / $250 = 50%.
        assert_eq!(value["tokenPercent"], 50.0);
        assert_eq!(value["dollarPercent"], 50.0);
    }

    #[test]
    fn json_dollar_percent_null_when_cost_unknown() {
        let limits = UsageLimits {
            monthly_token_limit: Some(1_000),
            monthly_dollar_limit: Some(100.0),
        };
        let json = usage_json(&[], UsageGroup::Day, &summary(500, None), &limits).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["tokenPercent"], 50.0);
        assert!(value["dollarPercent"].is_null());
    }

    #[test]
    fn text_report_shows_cache_column_and_excluded_note() {
        let rows = vec![UsageReportRow {
            date: Some("2026-07-12".to_string()),
            model_id: Some("claude-sonnet-4".to_string()),
            provider: Some("anthropic".to_string()),
            input_tokens: 1_000,
            output_tokens: 200,
            total_tokens: 1_800,
            cache_read_tokens: 500,
            cache_creation_tokens: 100,
            turns: 1,
            cost: Some(0.01),
            has_unpriced: false,
            cost_excludes_cache: true,
        }];
        let text = format_usage_text(
            &rows,
            UsageGroup::Day,
            &summary(1_800, Some(0.01)),
            &UsageLimits::default(),
            0,
            0,
        );
        assert!(text.contains("CACHE"), "header has a CACHE column: {text}");
        // cache_read + cache_creation = 600 rendered in the row.
        assert!(text.contains("600"), "{text}");
        assert!(
            text.contains("no cache pricing"),
            "the cost-excludes-cache note appears: {text}"
        );
    }

    #[test]
    fn parse_local_date_rejects_garbage() {
        assert!(parse_local_date("not-a-date", false).is_err());
        assert!(parse_local_date("2026-07-10", false).is_ok());
        // End-of-day is strictly later than start-of-day for the same date.
        let start = parse_local_date("2026-07-10", false).unwrap();
        let end = parse_local_date("2026-07-10", true).unwrap();
        assert!(end > start);
        assert_eq!(end - start, 86_399);
    }
}
