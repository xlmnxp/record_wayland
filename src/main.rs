use gst::prelude::*;
use gstreamer as gst;
use std::collections::HashMap;
use zbus::{
    dbus_proxy,
    export::futures_util::TryStreamExt,
    zvariant::{ObjectPath, OwnedObjectPath, Structure, Value},
    Connection, MessageStream, MessageType, Result,
};

#[dbus_proxy(
    interface = "org.freedesktop.portal.ScreenCast",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait ScreenCast {
    async fn create_session(&self, options: HashMap<&str, Value<'_>>) -> Result<OwnedObjectPath>;
    async fn select_sources(
        &self,
        session_handle: ObjectPath<'_>,
        options: HashMap<&str, Value<'_>>,
    ) -> Result<OwnedObjectPath>;
    async fn start(
        &self,
        session_handle: ObjectPath<'_>,
        parent_window: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> Result<OwnedObjectPath>;
}

async fn handle_session(
    screen_cast_proxy: ScreenCastProxy<'_>,
    response: HashMap<&str, Value<'_>>,
) -> Result<()> {
    let response_session_handle = response
        .get("session_handle")
        .expect("cannot get session_handle")
        .clone()
        .downcast::<String>()
        .expect("cannot down cast session_handle");

    screen_cast_proxy
        .select_sources(
            ObjectPath::try_from(response_session_handle.clone())?,
            HashMap::from([("handle_token", Value::from("blue_recorder_1"))]),
        )
        .await?;

    screen_cast_proxy
        .start(
            ObjectPath::try_from(response_session_handle.clone())?,
            "parent_window",
            HashMap::from([("handle_token", Value::from("blue_recorder_1"))]),
        )
        .await?;
    Ok(())
}

async fn record_screen_cast(response: HashMap<&str, Value<'_>>) -> Result<()> {
    let streams: &Value<'_> = response.get("streams").expect("cannot get streams");

    // get fields from nested structure inside elements
    // NOTICE: this is not the best way to get node_id, but it works for now
    let stream_node_id: u32 = streams
        .clone()
        .downcast::<Vec<Value>>()
        .expect("cannot down cast streams to vec array")
        .get(0)
        .expect("cannot get first object from streams array")
        .clone()
        .downcast::<Structure>()
        .expect("cannot down cast first object to structure")
        .fields()
        .get(0)
        .expect("cannot get first field from structure")
        .clone()
        .downcast::<u32>()
        .expect("cannot down cast first field to u32");

    // launch gstreamer pipeline
    let gst_element: gst::Element = gst::parse_launch(&format!(
            "pipewiresrc do-timestamp=true keepalive-time=1000 path={stream_node_id} ! videorate ! video/x-raw,framerate=30/1 ! videoconvert chroma-mode=none dither=none matrix-mode=output-only ! queue ! vp8enc cpu-used=16 max-quantizer=17 deadline=1 keyframe-mode=disabled threads=8 static-threshold=1000 buffer-size=20000 ! queue ! webmmux ! filesink location=test.mp4"
        )).expect("failed to launch gstreamer pipeline");

    // start pipeline
    let pipeline: gst::Pipeline = gst_element
        .dynamic_cast::<gst::Pipeline>()
        .expect("pipeline error");

    pipeline
        .set_state(gst::State::Playing)
        .expect("failed to start pipeline");

    println!("Recording Wayland screen cast...");
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    gst::init().expect("Failed to initialize GStreamer.");
    let connection = Connection::session().await?;
    let screen_cast_proxy: ScreenCastProxy<'_> = ScreenCastProxy::new(&connection).await?;

    screen_cast_proxy
        .create_session(HashMap::from([
            ("handle_token", Value::from("blue_recorder_1")),
            ("session_handle_token", Value::from("blue_recorder_1")),
        ]))
        .await?;

    let mut message_stream = MessageStream::from(connection);

    while let Some(msg) = message_stream.try_next().await? {
        match msg.message_type() {
            MessageType::Signal => {
                let (_, response) = msg.body::<(u32, HashMap<&str, Value>)>()?;
                if response.len() == 0 {
                    continue;
                }

                if response.contains_key("session_handle") {
                    handle_session(screen_cast_proxy.clone(), response.clone()).await?;
                }

                if response.contains_key("streams") {
                    // TODO: start recording on separate thread
                    record_screen_cast(response.clone()).await?;
                }
            }
            MessageType::MethodReturn => {
                println!("\n\nMethodReturn message: {:?}", msg);
            }
            _ => {
                println!("\n\nUnkown message: {:?}", msg);
            }
        }
    }

    Ok(())
}
