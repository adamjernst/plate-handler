#[derive(Debug)]
pub struct SpottedPlate {
    pub plate: String,
    pub score: f64,
    pub vehicle_type: String,
    pub image_url: Option<String>,
}
