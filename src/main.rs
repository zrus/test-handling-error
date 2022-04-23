use anyhow::Error;
use derive_more::{Display, Error};
use gst::prelude::{Cast, ElementExt, GObjectExtManualGst, GstBinExt, GstObjectExt};

use std::env;
#[derive(Debug, Display, Error)]
#[display(fmt = "Received error from {}: {} (debug: {:?})", src, error, debug)]
struct ErrorMessage {
    src: String,
    error: String,
    debug: Option<String>,
    source: gst::glib::Error,
}

#[derive(Debug)]
enum GStreamerError {
    Retriable(String),
    Irretriable(String),
}

impl From<gst::glib::Error> for GStreamerError {
    fn from(err: gst::glib::Error) -> Self {
        unsafe {
            let raw = err.into_raw();
            match (*raw).domain {
                2434 => Self::Irretriable(String::from("URI contains invalid protocol")),
                2626 => match (*raw).code {
                    5 => Self::Irretriable(String::from("URI contains invalid path")),
                    7 => Self::Irretriable(String::from("URI contains invalid domain")),
                    _ => Self::Irretriable(String::from("URI invalid")),
                },
                2628 => Self::Irretriable(String::from("Unauthorized")),
                2593 => Self::Irretriable(String::from("URI contains invalid port")),
                _ => Self::Irretriable(String::from("Unhandled error")),
            }
        }
    }
}

fn main() {
    let mut args = env::args();
    let _arg0 = args.next().unwrap();

    let uri = args.next().expect("No input URI provided");
    let username = args.next();
    let password = args.next();
    let num_thread = args.next();
    let num_thread = num_thread.unwrap().parse::<u8>().unwrap();

    let mut handlers = Vec::new();

    for i in 0..num_thread {
        let uri = uri.clone();
        let username = username.clone();
        let password = password.clone();
        handlers.push(std::thread::spawn(move || {
            match create_pipeline(&uri, username.as_deref(), password.as_deref(), i)
                .and_then(main_loop)
            {
                Ok(r) => r,
                // Err(e) => eprintln!("Error: {}", e),
                Err(_) => {}
            }
        }))
    }

    for handler in handlers {
        handler.join().unwrap();
    }
}

/// Wrong domain, wrong path, wrong port, wrong protocol, wrong authen info are need to stop absolutely.
/// Most of others else needs to be retried.

fn create_pipeline(
    uri: &str,
    username: Option<&str>,
    password: Option<&str>,
    seq: u8,
) -> Result<gst::Pipeline, Error> {
    gst::init()?;

    println!("uri: {}", uri);

    let pipeline = gst::parse_launch(&format!("rtspsrc name=src location={} ! appsink name=sink max-buffers=5 drop=true sync=true wait-on-eos=false", uri))?
        .downcast::<gst::Pipeline>()
        .expect("");

    if username.is_some() && password.is_some() {
        println!("authen: {} {}", username.unwrap(), password.unwrap());
        let src = pipeline.by_name("src").unwrap();
        src.set_property_from_str("user-id", username.unwrap());
        src.set_property_from_str("user-pw", password.unwrap());
    }

    let sink = pipeline
        .by_name("sink")
        .expect("expected for element appsink")
        .downcast::<gst_app::AppSink>()
        .expect("expected for appsink object");

    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |appsink| match appsink.pull_sample() {
                Ok(_) => {
                    println!("thread {}: ok", seq);
                    Ok(gst::FlowSuccess::Ok)
                }
                Err(_) => {
                    eprintln!("thread {}: error", seq);
                    Err(gst::FlowError::Error)
                }
            })
            .build(),
    );

    Ok(pipeline)
}

fn main_loop(pipeline: gst::Pipeline) -> Result<(), Error> {
    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().expect("");

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(eos) => {
                println!("{:?}", eos);
                println!("Got Eos msg, done");
                break;
            }
            MessageView::Error(err) => {
                pipeline.set_state(gst::State::Null)?;
                eprintln!("{:?}", GStreamerError::from(err.error()));
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
