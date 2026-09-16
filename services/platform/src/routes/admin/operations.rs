use super::*;

pub(super) async fn get_operation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BackgroundOperation>, ApiError> {
    let operation = state
        .get_operation(id)
        .await?
        .filter(|operation| operation.kind == OperationKind::ProductImport)
        .ok_or_else(|| ApiError::not_found("Product import operation was not found."))?;
    Ok(Json(operation))
}

pub(super) async fn operation_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let operation = state
        .get_operation(id)
        .await?
        .filter(|operation| operation.kind == OperationKind::ProductImport)
        .ok_or_else(|| ApiError::not_found("Product import operation was not found."))?;
    let updates = stream::unfold(
        (state, id, Some(operation), false),
        |(state, id, initial, finished)| async move {
            if finished {
                return None;
            }
            let operation = match initial {
                Some(operation) => Some(operation),
                None => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    state.get_operation(id).await.ok().flatten()
                }
            };
            let Some(operation) = operation else {
                let event = Event::default()
                    .event("error")
                    .data("Operation status is temporarily unavailable.");
                return Some((Ok(event), (state, id, None, true)));
            };
            let finished = matches!(
                operation.status,
                OperationStatus::Completed | OperationStatus::Failed
            );
            let event = Event::default()
                .event("operation")
                .json_data(&operation)
                .unwrap_or_else(|_| {
                    Event::default()
                        .event("error")
                        .data("Operation status serialization failed.")
                });
            Some((Ok(event), (state, id, None, finished)))
        },
    );
    Ok(Sse::new(updates).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("operation-stream"),
    ))
}
