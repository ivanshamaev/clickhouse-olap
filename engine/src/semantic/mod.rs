//! Semantic model: loads TOML model configs and exposes metadata and validation helpers.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::api::types::{
    AggType, DimensionMeta, GuardsMeta, MeasureMetaModel, ModelMetadata, ModelSummary,
};

// ── TOML config structs ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    pub id: String,
    pub label: Option<String>,
    pub source: SourceConfig,
    #[serde(default)]
    pub dimensions: Vec<DimensionConfig>,
    #[serde(default)]
    pub measures: Vec<MeasureConfig>,
    #[serde(default)]
    pub filterable_fields: Vec<String>,
    #[serde(default)]
    pub guards: GuardsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SourceConfig {
    /// ClickHouse table name (optionally qualified: `db.table`).
    pub table: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DimensionConfig {
    pub field: String,
    pub label: Option<String>,
    #[serde(rename = "type", default = "default_dim_type")]
    pub dim_type: String,
    #[serde(default)]
    pub hierarchy: Vec<String>,
}

fn default_dim_type() -> String {
    "categorical".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct MeasureConfig {
    pub id: String,
    pub field: String,
    pub label: Option<String>,
    pub allowed_agg: Vec<String>,
    #[serde(default = "default_true")]
    pub additive: bool,
    pub format: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GuardsConfig {
    #[serde(default = "default_max_groups")]
    pub max_groups: u64,
    #[serde(default)]
    pub high_cardinality_fields: Vec<String>,
}

fn default_max_groups() -> u64 {
    200_000
}

// ── SemanticModel (runtime) ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SemanticModel {
    pub config: ModelConfig,
    /// Dimension map: field → config
    dimensions: HashMap<String, DimensionConfig>,
    /// Measure map: id → config
    measures: HashMap<String, MeasureConfig>,
}

impl SemanticModel {
    pub fn new(config: ModelConfig) -> Self {
        let dimensions = config
            .dimensions
            .iter()
            .map(|d| (d.field.clone(), d.clone()))
            .collect();
        let measures = config
            .measures
            .iter()
            .map(|m| (m.id.clone(), m.clone()))
            .collect();
        Self {
            config,
            dimensions,
            measures,
        }
    }

    pub fn id(&self) -> &str {
        &self.config.id
    }

    pub fn table(&self) -> &str {
        &self.config.source.table
    }

    pub fn dimension(&self, field: &str) -> Option<&DimensionConfig> {
        self.dimensions.get(field)
    }

    pub fn measure(&self, id: &str) -> Option<&MeasureConfig> {
        self.measures.get(id)
    }

    /// Check whether `field` is allowed as a dimension (in rows/columns).
    pub fn validate_dimension_field(&self, field: &str) -> bool {
        self.dimensions.contains_key(field)
    }

    /// Check whether `field` is allowed in filters.
    pub fn validate_filter_field(&self, field: &str) -> bool {
        self.config.filterable_fields.contains(&field.to_string())
            || self.dimensions.contains_key(field)
    }

    /// Check whether `agg` is allowed for measure `id`.
    pub fn validate_measure_agg(&self, id: &str, agg: &AggType) -> bool {
        let agg_str = serde_json::to_value(agg)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        self.measures
            .get(id)
            .map(|m| m.allowed_agg.contains(&agg_str))
            .unwrap_or(false)
    }

    pub fn guards(&self) -> &GuardsConfig {
        &self.config.guards
    }

    /// Convert to API metadata type.
    pub fn to_metadata(&self) -> ModelMetadata {
        ModelMetadata {
            model_id: self.config.id.clone(),
            label: self.config.label.clone(),
            dimensions: self
                .config
                .dimensions
                .iter()
                .map(|d| DimensionMeta {
                    field: d.field.clone(),
                    label: d.label.clone(),
                    dim_type: d.dim_type.clone(),
                    hierarchy: if d.hierarchy.is_empty() {
                        None
                    } else {
                        Some(d.hierarchy.clone())
                    },
                })
                .collect(),
            measures: self
                .config
                .measures
                .iter()
                .map(|m| MeasureMetaModel {
                    id: m.id.clone(),
                    field: m.field.clone(),
                    label: m.label.clone(),
                    allowed_agg: m.allowed_agg.clone(),
                    additive: m.additive,
                    format: m.format.clone(),
                })
                .collect(),
            filterable_fields: self.config.filterable_fields.clone(),
            guards: GuardsMeta {
                max_groups: self.config.guards.max_groups,
                high_cardinality_fields: self.config.guards.high_cardinality_fields.clone(),
            },
        }
    }

    pub fn to_summary(&self) -> ModelSummary {
        ModelSummary {
            model_id: self.config.id.clone(),
            label: self.config.label.clone(),
        }
    }
}

// ── ModelStore ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ModelStore {
    models: HashMap<String, SemanticModel>,
}

impl ModelStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load all `*.toml` files from `dir`.
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let mut store = Self::new();
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("reading models dir: {}", dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading model file: {}", path.display()))?;
            let config: ModelConfig = toml::from_str(&text)
                .with_context(|| format!("parsing model TOML: {}", path.display()))?;
            let model = SemanticModel::new(config);
            tracing::info!(model_id = %model.id(), "loaded semantic model");
            store.models.insert(model.id().to_string(), model);
        }

        Ok(store)
    }

    pub fn get(&self, id: &str) -> Option<&SemanticModel> {
        self.models.get(id)
    }

    pub fn list(&self) -> Vec<ModelSummary> {
        let mut summaries: Vec<_> = self.models.values().map(|m| m.to_summary()).collect();
        summaries.sort_by(|a, b| a.model_id.cmp(&b.model_id));
        summaries
    }
}
