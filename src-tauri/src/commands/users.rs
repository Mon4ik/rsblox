use crate::types::{ClientInfo, RobloxApiState};
use roboat::{friends::FriendUserInformation, ClientBuilder};
use tauri::State;

#[tauri::command]
pub async fn get_me(state: State<'_, RobloxApiState>) -> Result<ClientInfo, String> {
    let cookie = { state.0.lock().unwrap().clone() };

    let client = ClientBuilder::new().roblosecurity(cookie).build();

    Ok(ClientInfo {
        user_id: client.user_id().await.map_err(|err| err.to_string())?,
        username: client.username().await.map_err(|err| err.to_string())?,
        display_name: client.display_name().await.map_err(|err| err.to_string())?,
        robux: client.robux().await.map_err(|err| err.to_string())?,
    })
}

#[tauri::command]
pub async fn friends_list(
    state: State<'_, RobloxApiState>,
) -> Result<Vec<FriendUserInformation>, String> {
    let cookie = { state.0.lock().unwrap().clone() };

    let client = ClientBuilder::new().roblosecurity(cookie).build();

    let user_id = match client.user_id().await {
        Ok(v) => v,
        Err(err) => return Err(err.to_string()),
    };

    client
        .friends_list(user_id)
        .await
        .map_err(|err| err.to_string())
}
