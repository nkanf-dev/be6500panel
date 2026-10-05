use std::path::Path;
fn main() {
    let catalog = be6500_panel::features_gateway::Features::open(Path::new(
        "/absolute/synthetic-feature-data",
    ))
    .catalog();
    println!("{}", serde_json::to_string_pretty(&catalog).unwrap());
}
