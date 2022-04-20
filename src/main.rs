use anyhow::Error;
use derive_more::{Display, Error};
use gst::prelude::{Cast, ElementExt, GObjectExtManualGst, GstBinExt, GstObjectExt};

#[derive(Debug, Display, Error)]
#[display(fmt = "Received error from {}: {} (debug: {:?})", src, error, debug)]
struct ErrorMessage {
    src: String,
    error: String,
    debug: Option<String>,
    source: gst::glib::Error,
}

fn main() {
    use std::env;

    let mut args = env::args();
    let _arg0 = args.next().unwrap();

    let uri = args.next().expect("No input URI provided");
    let username = args.next();
    let password = args.next();

    match create_pipeline(&uri, username.as_deref(), password.as_deref()).and_then(main_loop) {
        Ok(r) => r,
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn create_pipeline(
    uri: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<gst::Pipeline, Error> {
    gst::init()?;

    let pipeline = gst::parse_launch(&format!(
        "gst-launch-1.0 rtspsrc name=src location='{}' ! fakesink",
        uri
    ))?
    .downcast::<gst::Pipeline>()
    .expect("");

    if username.is_some() && password.is_some() {
        let src = pipeline.by_name("src").unwrap();
        src.set_property_from_str("user-id", username.unwrap());
        src.set_property_from_str("user-pw", password.unwrap());
    }

    Ok(pipeline)
}

fn main_loop(pipeline: gst::Pipeline) -> Result<(), Error> {
    pipeline.set_state(gst::State::Paused)?;
    let bus = pipeline.bus().expect("");

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(..) => {
                println!("Got Eos msg, done");
                break;
            }
            MessageView::Error(err) => {
                pipeline.set_state(gst::State::Null)?;
                return Err(ErrorMessage {
                    src: msg
                        .src()
                        .map(|s| String::from(s.path_string()))
                        .unwrap_or_else(|| String::from("None")),
                    error: err.error().to_string(),
                    debug: err.debug(),
                    source: err.error(),
                }
                .into());
            }
            _ => (),
        }
    }

    pipeline.set_state(gst::State::Null)?;

    Ok(())
}
