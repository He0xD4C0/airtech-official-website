fn main() {
    let document = airtek_http::openapi::document();
    println!(
        "{}",
        serde_json::to_string_pretty(&document).expect("OpenAPI document must serialize")
    );
}
