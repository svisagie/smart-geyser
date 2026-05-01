use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::app_state::AppState;

pub async fn get_events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let current = state.last_status_event.read().await.clone();
    let rx = state.events_tx.subscribe();

    // Send the most recent status immediately so the client is not left waiting
    // for the next scheduler tick, then stream all subsequent broadcasts.
    let initial = tokio_stream::iter(
        current
            .into_iter()
            .map(|json| Ok(Event::default().data(json))),
    );
    let live = BroadcastStream::new(rx)
        .filter_map(|result| result.ok().map(|json| Ok(Event::default().data(json))));

    Sse::new(initial.chain(live)).keep_alive(KeepAlive::default())
}
