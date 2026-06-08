#[derive(Debug, Clone)]
pub struct WalkerTraceRow {
    pub step: usize,
    pub walker_id: String,
    pub accepted: String,
    pub energy: Option<f64>,
    pub best_energy: Option<f64>,
    pub temperature: Option<f64>,
    pub step_size: Option<f64>,
    pub move_class: String,
    pub label: String,
}
