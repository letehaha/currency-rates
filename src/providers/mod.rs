mod ecb;
mod nbu;
mod provider;

pub use ecb::EcbProvider;
pub use nbu::NbuProvider;
pub use provider::{Provider, ProviderRegistry};

use crate::models::DailyRates;

/// Fill gaps in rate data (weekends, holidays) with the previous available day's rates.
/// Useful for providers like ECB that don't publish on weekends.
pub fn fill_gaps(mut rates: Vec<DailyRates>, provider_name: &str) -> Vec<DailyRates> {
    if rates.is_empty() {
        return rates;
    }

    // Sort by date ascending
    rates.sort_by_key(|r| r.date);

    let mut filled: Vec<DailyRates> = Vec::new();
    let mut prev_rates: Option<&DailyRates> = None;

    for (i, current) in rates.iter().enumerate() {
        // Fill gaps between previous and current date
        if let Some(prev) = prev_rates {
            let mut fill_date = prev.date + chrono::Duration::days(1);
            while fill_date < current.date {
                filled.push(DailyRates {
                    date: fill_date,
                    base_currency: prev.base_currency.clone(),
                    rates: prev.rates.clone(),
                    provider: provider_name.to_string(),
                });
                fill_date += chrono::Duration::days(1);
            }
        }

        filled.push(current.clone());
        prev_rates = Some(&rates[i]);
    }

    // Fill up to today if the last rate is not today
    if let Some(last) = filled.last() {
        let today = chrono::Utc::now().date_naive();
        let mut fill_date = last.date + chrono::Duration::days(1);
        let last_rates = last.rates.clone();
        let last_base = last.base_currency.clone();

        while fill_date <= today {
            filled.push(DailyRates {
                date: fill_date,
                base_currency: last_base.clone(),
                rates: last_rates.clone(),
                provider: provider_name.to_string(),
            });
            fill_date += chrono::Duration::days(1);
        }
    }

    filled
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn make_daily_rates(date: NaiveDate, rate_value: f64) -> DailyRates {
        let mut rates = HashMap::new();
        rates.insert("EUR".to_string(), rate_value);
        rates.insert("GBP".to_string(), rate_value * 0.8);
        DailyRates {
            date,
            base_currency: "USD".to_string(),
            rates,
            provider: "test".to_string(),
        }
    }

    #[test]
    fn test_fill_gaps_empty_input() {
        let result = fill_gaps(vec![], "test");
        assert!(result.is_empty());
    }

    #[test]
    fn test_fill_gaps_single_entry() {
        let date = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let rates = vec![make_daily_rates(date, 1.0)];

        let result = fill_gaps(rates, "test");

        // Should have original entry plus fill to today
        assert!(!result.is_empty());
        assert_eq!(result[0].date, date);
    }

    #[test]
    fn test_fill_gaps_no_gaps() {
        // Three consecutive days - no gaps to fill
        let rates = vec![
            make_daily_rates(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(), 1.0),
            make_daily_rates(NaiveDate::from_ymd_opt(2020, 1, 2).unwrap(), 1.1),
            make_daily_rates(NaiveDate::from_ymd_opt(2020, 1, 3).unwrap(), 1.2),
        ];

        let result = fill_gaps(rates, "test");

        // First 3 entries should be the originals
        assert_eq!(result[0].date, NaiveDate::from_ymd_opt(2020, 1, 1).unwrap());
        assert_eq!(result[1].date, NaiveDate::from_ymd_opt(2020, 1, 2).unwrap());
        assert_eq!(result[2].date, NaiveDate::from_ymd_opt(2020, 1, 3).unwrap());
        // Rates should be preserved
        assert_eq!(result[0].rates.get("EUR"), Some(&1.0));
        assert_eq!(result[1].rates.get("EUR"), Some(&1.1));
        assert_eq!(result[2].rates.get("EUR"), Some(&1.2));
    }

    #[test]
    fn test_fill_gaps_weekend_gap() {
        // Friday and Monday - should fill Saturday and Sunday
        let friday = NaiveDate::from_ymd_opt(2020, 1, 3).unwrap(); // Friday
        let monday = NaiveDate::from_ymd_opt(2020, 1, 6).unwrap(); // Monday

        let rates = vec![make_daily_rates(friday, 1.0), make_daily_rates(monday, 1.1)];

        let result = fill_gaps(rates, "test");

        // Should have: Friday, Saturday, Sunday, Monday, + fill to today
        assert_eq!(result[0].date, friday);
        assert_eq!(result[1].date, NaiveDate::from_ymd_opt(2020, 1, 4).unwrap()); // Saturday
        assert_eq!(result[2].date, NaiveDate::from_ymd_opt(2020, 1, 5).unwrap()); // Sunday
        assert_eq!(result[3].date, monday);

        // Saturday and Sunday should have Friday's rates
        assert_eq!(result[1].rates.get("EUR"), Some(&1.0));
        assert_eq!(result[2].rates.get("EUR"), Some(&1.0));
        // Monday should have its own rate
        assert_eq!(result[3].rates.get("EUR"), Some(&1.1));
    }

    #[test]
    fn test_fill_gaps_unsorted_input() {
        // Input is not sorted - should be sorted first
        let day1 = NaiveDate::from_ymd_opt(2020, 1, 3).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let day3 = NaiveDate::from_ymd_opt(2020, 1, 2).unwrap();

        let rates = vec![
            make_daily_rates(day1, 1.2), // Jan 3
            make_daily_rates(day2, 1.0), // Jan 1
            make_daily_rates(day3, 1.1), // Jan 2
        ];

        let result = fill_gaps(rates, "test");

        // Should be sorted by date
        assert_eq!(result[0].date, NaiveDate::from_ymd_opt(2020, 1, 1).unwrap());
        assert_eq!(result[1].date, NaiveDate::from_ymd_opt(2020, 1, 2).unwrap());
        assert_eq!(result[2].date, NaiveDate::from_ymd_opt(2020, 1, 3).unwrap());
        // With correct rates
        assert_eq!(result[0].rates.get("EUR"), Some(&1.0));
        assert_eq!(result[1].rates.get("EUR"), Some(&1.1));
        assert_eq!(result[2].rates.get("EUR"), Some(&1.2));
    }

    #[test]
    fn test_fill_gaps_large_gap() {
        // 5 day gap
        let day1 = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2020, 1, 7).unwrap(); // 6 days later

        let rates = vec![make_daily_rates(day1, 1.0), make_daily_rates(day2, 2.0)];

        let result = fill_gaps(rates, "test");

        // Should have 7 entries for days 1-7, plus fill to today
        assert_eq!(result[0].date, day1);
        assert_eq!(result[1].date, NaiveDate::from_ymd_opt(2020, 1, 2).unwrap());
        assert_eq!(result[2].date, NaiveDate::from_ymd_opt(2020, 1, 3).unwrap());
        assert_eq!(result[3].date, NaiveDate::from_ymd_opt(2020, 1, 4).unwrap());
        assert_eq!(result[4].date, NaiveDate::from_ymd_opt(2020, 1, 5).unwrap());
        assert_eq!(result[5].date, NaiveDate::from_ymd_opt(2020, 1, 6).unwrap());
        assert_eq!(result[6].date, day2);

        // Gap days should have day1's rates
        for item in result.iter().take(6).skip(1) {
            assert_eq!(item.rates.get("EUR"), Some(&1.0));
        }
        // Last original day should have its own rate
        assert_eq!(result[6].rates.get("EUR"), Some(&2.0));
    }

    #[test]
    fn test_fill_gaps_sets_provider_name_on_filled_entries() {
        let rates = vec![
            make_daily_rates(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(), 1.0),
            make_daily_rates(NaiveDate::from_ymd_opt(2020, 1, 3).unwrap(), 1.1),
        ];

        let result = fill_gaps(rates, "custom_provider");

        // Original entries keep their provider
        assert_eq!(result[0].provider, "test");
        assert_eq!(result[2].provider, "test");
        // Gap-filled entry gets the new provider name
        assert_eq!(result[1].provider, "custom_provider");
    }

    #[test]
    fn test_fill_gaps_preserves_base_currency() {
        let mut rates = HashMap::new();
        rates.insert("EUR".to_string(), 0.9);

        let input = vec![
            DailyRates {
                date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
                base_currency: "GBP".to_string(),
                rates: rates.clone(),
                provider: "test".to_string(),
            },
            DailyRates {
                date: NaiveDate::from_ymd_opt(2020, 1, 3).unwrap(),
                base_currency: "GBP".to_string(),
                rates,
                provider: "test".to_string(),
            },
        ];

        let result = fill_gaps(input, "test");

        // Gap-filled entry should have same base currency
        assert_eq!(result[1].base_currency, "GBP");
    }
}
