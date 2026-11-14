use serde::Deserialize;

use crate::model::{
    SlackHuddleJoinReceipt, SlackHuddleJoinRequest, SlackHuddleLeaveRequest,
    SlackHuddleMediaCredentials,
};

use super::SlackApiClient;

const SLACK_HUDDLE_JOIN_METHOD: &str = "rooms.join";
const SLACK_HUDDLE_LEAVE_METHOD: &str = "rooms.leave";

#[derive(Deserialize)]
struct SlackHuddleJoinResponse {
    call: SlackHuddleCallResponse,
    huddle: Option<SlackHuddleResponse>,
    canvas: Option<SlackHuddleCanvasResponse>,
}

#[derive(Deserialize)]
struct SlackHuddleCallResponse {
    call_id: String,
    free_willy: SlackHuddleMediaCredentials,
}

#[derive(Deserialize)]
struct SlackHuddleResponse {
    #[serde(default)]
    channels: Vec<String>,
}

#[derive(Deserialize)]
struct SlackHuddleCanvasResponse {
    root_thread_ts: Option<crate::model::SlackMessageTimestamp>,
    canvas_file_id: Option<String>,
}

impl SlackApiClient {
    pub fn join_huddle(
        &self,
        request: &SlackHuddleJoinRequest,
    ) -> Result<SlackHuddleJoinReceipt, String> {
        let mut params = vec![
            ("channel_id", request.conversation_id.clone()),
            ("regions", request.region.clone()),
            ("multidevice", request.multidevice.to_string()),
        ];
        if let Some(room_id) = request.room_id.as_ref() {
            params.push(("id", room_id.clone()));
        }
        if let Some(thread_timestamp) = request.thread_timestamp.as_ref() {
            params.push(("thread_ts", thread_timestamp.as_str().to_string()));
        }

        let payload = self.post(SLACK_HUDDLE_JOIN_METHOD, &params)?;
        parse_join_response(payload)
    }

    pub fn leave_huddle(&self, request: &SlackHuddleLeaveRequest) -> Result<(), String> {
        let params = [
            ("channel_id", request.conversation_id.clone()),
            ("call_id", request.call_id.clone()),
            ("attendee_id", request.attendee_id.clone()),
            ("reason", request.reason.clone()),
        ];
        match self.post(SLACK_HUDDLE_LEAVE_METHOD, &params) {
            Ok(_) => Ok(()),
            Err(error) if benign_huddle_leave_error(&error) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn parse_join_response(payload: serde_json::Value) -> Result<SlackHuddleJoinReceipt, String> {
    let response = serde_json::from_value::<SlackHuddleJoinResponse>(payload)
        .map_err(|error| format!("Slack rooms.join response is invalid: {error}"))?;
    Ok(SlackHuddleJoinReceipt {
        call_id: response.call.call_id,
        conversation_id: response
            .huddle
            .and_then(|huddle| huddle.channels.into_iter().next()),
        root_thread_timestamp: response
            .canvas
            .as_ref()
            .and_then(|canvas| canvas.root_thread_ts.clone()),
        canvas_file_id: response.canvas.and_then(|canvas| canvas.canvas_file_id),
        media: response.call.free_willy,
    })
}

fn benign_huddle_leave_error(error: &str) -> bool {
    [
        "channel_not_found",
        "room_not_found",
        "invalid_channel_id",
        "attendee_not_found",
    ]
    .iter()
    .any(|code| error.ends_with(code))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_join_response;

    #[test]
    fn a_rooms_join_answer_yields_the_chime_meeting_and_attendee() {
        let receipt = parse_join_response(json!({
            "ok": true,
            "call": {
                "call_id": "R123",
                "free_willy": {
                    "meeting": {
                        "MeetingId": "m-1",
                        "ExternalMeetingId": "R123",
                        "MediaRegion": "us-east-1",
                        "MediaPlacement": {
                            "AudioHostUrl": "audio.example:3478",
                            "SignalingUrl": "wss://signal.example/control/m-1",
                            "TurnControlUrl": "https://turn.example/v2/turn_sessions"
                        }
                    },
                    "attendee": {
                        "AttendeeId": "a-1",
                        "ExternalUserId": "U1",
                        "JoinToken": "token"
                    }
                }
            },
            "huddle": {"channels": ["C1"]}
        }))
        .unwrap();
        assert_eq!(receipt.call_id, "R123");
        assert_eq!(receipt.conversation_id.as_deref(), Some("C1"));
        let placement = &receipt.media.meeting.media_placement;
        assert_eq!(placement.signaling_url, "wss://signal.example/control/m-1");
        assert_eq!(placement.audio_host_url, "audio.example:3478");
        assert_eq!(receipt.media.attendee.attendee_id, "a-1");
        assert_eq!(receipt.media.attendee.join_token, "token");
    }
}
