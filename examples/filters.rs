#[macro_use]
extern crate rustful;

use std::sync::RwLock;
use std::error::Error;

#[macro_use]
extern crate log;
extern crate env_logger;

use rustful::{Server, TreeRouter, Context, Response};
use rustful::filter::{FilterContext, ResponseFilter, ResponseAction, ContextFilter, ContextAction};
use rustful::response::Data;
use rustful::StatusCode;
use rustful::header::Headers;
use rustful::header::ContentType;
use rustful::context::{Uri, MaybeUtf8Owned};

fn say_hello(mut context: Context, mut response: Response, format: &Format) {
    //Take the name of the JSONP function from the query variables
    let mut quote_msg = if let Some(jsonp_name) = context.query.remove("jsonp") {
        response.filter_storage_mut().insert(JsonpFn(jsonp_name.into()));
        true
    } else {
        false
    };

    //Is the format supposed to be a JSON structure? Then set a variable name/content type
    if let Format::Json = *format {
        let mime = ContentType(content_type!(Application / Json; Charset = Utf8));
        response.headers_mut().set(mime);
        response.filter_storage_mut().insert(JsonVar("message"));
        quote_msg = true;
    }

    let person = match context.variables.get("person") {
        Some(name) => name,
        None => "stranger".into()
    };

    let message = if quote_msg {
        format!("\"Hello, {}!\"", person)
    } else {
        format!("Hello, {}!", person)
    };

    //Using `try_send` allows us to catch eventual errors from the filters.
    //This example should not produce any errors, so this is only for show.
    if let Err(e) = response.try_send(message) {
        error!("could not send hello: {}", e.description());
    }
}

enum Format {
    Json,
    Text
}

struct Handler(fn(Context, Response, &Format), Format);

impl rustful::Handler for Handler {
    fn handle_request(&self, context: Context, response: Response) {
        self.0(context, response, &self.1);
    }
}

fn main() {
    env_logger::init().unwrap();

    println!("Visit http://localhost:8080, http://localhost:8080/Peter or http://localhost:8080/json/Peter (if your name is Peter) to try this example.");
    println!("Append ?jsonp=someFunction to get a JSONP response.");
    println!("Run this example with the environment variable 'RUST_LOG' set to 'debug' to see the debug prints.");

    let mut router = TreeRouter::new();
    insert_routes!{
        &mut router => {
            "print" => {
                Get: Handler(say_hello, Format::Text),
                ":person" => Get: Handler(say_hello, Format::Text),

                "json" => {
                    Get: Handler(say_hello, Format::Json),
                    ":person" => Get: Handler(say_hello, Format::Json)
                }
            }
        }
    };

    let server_result = Server {
        host: 8080.into(),
        handlers: router,

        //Log path, change path, log again
        context_filters: vec![
            Box::new(RequestLogger::new()),
            Box::new(PathPrefix::new("print")),
            Box::new(RequestLogger::new())
        ],

        response_filters: vec![Box::new(Jsonp), Box::new(Json)],

        ..Server::default()
    }.run();

    match server_result {
        Ok(_server) => {},
        Err(e) => error!("could not start server: {}", e.description())
    }
}

struct RequestLogger {
    counter: RwLock<u32>
}

impl RequestLogger {
    pub fn new() -> RequestLogger {
        RequestLogger {
            counter: RwLock::new(0)
        }
    }
}

impl ContextFilter for RequestLogger {
    ///Count requests and log the path.
    fn modify(&self, _ctx: FilterContext, context: &mut Context) -> ContextAction {
        *self.counter.write().unwrap() += 1;
        debug!("Request #{} is to '{}'", *self.counter.read().unwrap(), context.uri);
        ContextAction::next()
    }
}


struct PathPrefix {
    prefix: &'static str
}

impl PathPrefix {
    pub fn new(prefix: &'static str) -> PathPrefix {
        PathPrefix {
            prefix: prefix
        }
    }
}

impl ContextFilter for PathPrefix {
    ///Append the prefix to the path
    fn modify(&self, _ctx: FilterContext, context: &mut Context) -> ContextAction {
        let new_uri = context.uri.as_path().map(|path| {
            let mut new_path = MaybeUtf8Owned::from("/");
            new_path.push_str(self.prefix.trim_matches('/'));
            new_path.push_bytes(path.as_ref());
            Uri::Path(new_path)
        });
        if let Some(uri) = new_uri {
            context.uri = uri;
        }
        ContextAction::next()
    }
}

struct JsonVar(&'static str);

struct Json;

impl ResponseFilter for Json {
    fn begin(&self, ctx: FilterContext, status: StatusCode, _headers: &mut Headers) -> (StatusCode, ResponseAction) {
        //Check if a JSONP function is defined and write the beginning of the call
        let output = if let Some(&JsonVar(var)) = ctx.storage.get() {
            Some(format!("{{\"{}\": ", var))
        } else {
            None
        };

        (status, ResponseAction::next(output))
    }

    fn write<'a>(&'a self, _ctx: FilterContext, bytes: Option<Data<'a>>) -> ResponseAction {
        ResponseAction::next(bytes)
    }

    fn end(&self, ctx: FilterContext) -> ResponseAction {
        //Check if a JSONP function is defined and write the end of the call
        let output = ctx.storage.get::<JsonVar>().map(|_| "}");
        ResponseAction::next(output)
    }
}

struct JsonpFn(String);

struct Jsonp;

impl ResponseFilter for Jsonp {
    fn begin(&self, ctx: FilterContext, status: StatusCode, _headers: &mut Headers) -> (StatusCode, ResponseAction) {
        //Check if a JSONP function is defined and write the beginning of the call
        let output = if let Some(&JsonpFn(ref function)) = ctx.storage.get() {
            Some(format!("{}(", function))
        } else {
            None
        };

        (status, ResponseAction::next(output))
    }

    fn write<'a>(&'a self, _ctx: FilterContext, bytes: Option<Data<'a>>) -> ResponseAction {
        ResponseAction::next(bytes)
    }

    fn end(&self, ctx: FilterContext) -> ResponseAction {
        //Check if a JSONP function is defined and write the end of the call
        let output = ctx.storage.get::<JsonpFn>().map(|_| ");");
        ResponseAction::next(output)
    }
}
