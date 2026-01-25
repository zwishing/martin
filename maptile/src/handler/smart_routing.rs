//! Smart routing logic for automatic filter function selection
//!
//! This module implements intelligent source selection based on query parameters.
//! When a request contains filter parameters, it automatically routes to the
//! filtered function variant (e.g., `cities` → `cities_filtered`).

use std::collections::HashMap;

use log::debug;

/// Query parameters that trigger filtered function routing
const FILTER_PARAMS: &[&str] = &[
    "limit",
    "offset",
    "sortby",
    "properties",
    "datetime",
    "datetime_from",
    "datetime_to",
    // Any parameter ending with _min or _max is considered a filter
];

/// Check if query parameters contain any filter parameters
pub fn has_filter_params(query_params: &HashMap<String, String>) -> bool {
    if query_params.is_empty() {
        return false;
    }

    // Check for known filter parameters
    for key in query_params.keys() {
        if FILTER_PARAMS.contains(&key.as_str()) {
            return true;
        }

        // Check for range filters (_min, _max suffixes)
        if key.ends_with("_min") || key.ends_with("_max") {
            return true;
        }

        // Any other parameter is considered a property filter
        // (e.g., population=1000000, name=Tokyo)
        return true;
    }

    false
}

/// Resolve source ID with smart routing
///
/// If query parameters contain filters and a filtered variant exists,
/// automatically route to the filtered function.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use maptile::handler::smart_routing::resolve_source_id;
///
/// let available_sources = vec!["cities".to_string(), "cities_filtered".to_string()];
///
/// // No filters → use base source
/// let source_id = resolve_source_id("cities", &HashMap::new(), &available_sources);
/// assert_eq!(source_id, "cities");
///
/// // With filters → use filtered variant if available
/// let mut params = HashMap::new();
/// params.insert("limit".to_string(), "100".to_string());
/// let source_id = resolve_source_id("cities", &params, &available_sources);
/// assert_eq!(source_id, "cities_filtered");
/// ```

pub fn resolve_source_id(
    requested_id: &str,
    query_params: &HashMap<String, String>,
    available_sources: &[String],
) -> String {
    // If no filter parameters, use the requested source as-is
    if !has_filter_params(query_params) {
        debug!("No filter params, using base source: {}", requested_id);
        return requested_id.to_string();
    }

    // Check if filtered variant exists
    let filtered_id = format!("{}_filtered", requested_id);
    if available_sources.iter().any(|s| s == &filtered_id) {
        debug!(
            "Filter params detected, routing {} → {}",
            requested_id, filtered_id
        );
        return filtered_id;
    }

    // Fallback to base source if filtered variant doesn't exist
    debug!(
        "Filter params detected but no filtered variant found for {}, using base source",
        requested_id
    );
    requested_id.to_string()
}

/// Resolve multiple source IDs with smart routing
pub fn resolve_source_ids(
    requested_ids: &[&str],
    query_params: &HashMap<String, String>,
    available_sources: &[String],
) -> Vec<String> {
    requested_ids
        .iter()
        .map(|id| resolve_source_id(id, query_params, available_sources))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_filter_params_empty() {
        let params = HashMap::new();
        assert!(!has_filter_params(&params));
    }

    #[test]
    fn test_has_filter_params_limit() {
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "100".to_string());
        assert!(has_filter_params(&params));
    }

    #[test]
    fn test_has_filter_params_range() {
        let mut params = HashMap::new();
        params.insert("population_min".to_string(), "1000000".to_string());
        assert!(has_filter_params(&params));
    }

    #[test]
    fn test_has_filter_params_property() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "Tokyo".to_string());
        assert!(has_filter_params(&params));
    }

    #[test]
    fn test_resolve_source_id_no_filters() {
        let params = HashMap::new();
        let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

        let result = resolve_source_id("cities", &params, &sources);
        assert_eq!(result, "cities");
    }

    #[test]
    fn test_resolve_source_id_with_filters() {
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "100".to_string());
        let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

        let result = resolve_source_id("cities", &params, &sources);
        assert_eq!(result, "cities_filtered");
    }

    #[test]
    fn test_resolve_source_id_no_filtered_variant() {
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "100".to_string());
        let sources = vec!["cities".to_string()]; // No filtered variant

        let result = resolve_source_id("cities", &params, &sources);
        assert_eq!(result, "cities"); // Fallback to base
    }

    #[test]
    fn test_resolve_multiple_source_ids() {
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "100".to_string());
        let sources = vec![
            "cities".to_string(),
            "cities_filtered".to_string(),
            "roads".to_string(),
            "roads_filtered".to_string(),
        ];

        let result = resolve_source_ids(&["cities", "roads"], &params, &sources);
        assert_eq!(result, vec!["cities_filtered", "roads_filtered"]);
    }
}
