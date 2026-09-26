use super::*;

// Called after the controlled 5,000-row draft catalog is created in a disposable clone.
pub async fn verify(state: AppState) {
    let pool = &state.pool;
    for (stable_id, family, motor) in [
        ("SYNTH-00001", "axial", "EC"),
        ("SYNTH-00002", "axial", "AC"),
        ("SYNTH-00003", "centrifugal", "EC"),
    ] {
        let mut payload: Value =
            sqlx::query_scalar("SELECT payload FROM products WHERE stable_id=$1")
                .bind(stable_id)
                .fetch_one(pool)
                .await
                .unwrap();
        payload["family"] = json!(family);
        payload["motorTechnology"] = json!(motor);
        payload["specifications"] = json!([
            {"key":"protection","label":"Protection","value":"IP54","unit":null,
             "operatingCondition":null,"state":"verified","sourceReference":"TEST"},
            {"key":"private","label":"Pending","value":"secret-value","unit":null,
             "operatingCondition":null,"state":"pendingVerification","sourceReference":"TEST"}
        ]);
        let id = Uuid::parse_str(payload["id"].as_str().unwrap()).unwrap();
        sqlx::query("UPDATE products SET status='published',published_revision=1,payload=$2,family=$3 WHERE id=$1")
            .bind(id).bind(&payload).bind(family).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO product_revisions(product_id,revision,source_snapshot_id,payload,created_at) SELECT id,1,source_snapshot_id,$2,now() FROM products WHERE id=$1")
            .bind(id).bind(&payload).execute(pool).await.unwrap();
        let slug = payload["slug"].as_str().unwrap();
        let seo = json!({"title":null,"description":null,"canonicalPath":format!("/en/products/{family}/{slug}"),"indexable":true});
        sqlx::query("INSERT INTO product_localizations(product_id,product_revision,locale,slug,title,seo_metadata,translation_state,indexable,updated_by) VALUES($1,1,'en',$2,'Published synthetic model',$3,'verified',true,'TEST')")
            .bind(id).bind(slug).bind(&seo).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO public_routes(id,entity_type,entity_id,locale,canonical_path,indexable) VALUES($1,'product',$2,'en',$3,true)")
            .bind(Uuid::new_v4()).bind(id).bind(seo["canonicalPath"].as_str()).execute(pool).await.unwrap();
    }
    let app = build_router(state);
    let fetch = |query: String| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(
                    Request::get(format!("/api/public/v1/products?{query}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            serde_json::from_slice::<Value>(
                &response.into_body().collect().await.unwrap().to_bytes(),
            )
            .unwrap()
        }
    };
    let filtered = fetch("q=ip54&family=axial&motorTechnology=EC&limit=1".into()).await;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["familyCounts"].as_array().unwrap().len(), 2);
    assert_eq!(
        filtered["motorTechnologyCounts"].as_array().unwrap().len(),
        2
    );
    let first = fetch("q=IP54&limit=1".into()).await;
    assert_eq!(first["total"], 3);
    let second = fetch(format!(
        "q=ip54&limit=1&cursor={}",
        first["nextCursor"].as_str().unwrap()
    ))
    .await;
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    assert_eq!(second["total"], 3);
    assert_eq!(fetch("q=secret-value".into()).await["total"], 0);
    assert_eq!(fetch("q=SYNTH-00003".into()).await["total"], 1);
}
