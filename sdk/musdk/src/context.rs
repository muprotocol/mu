use std::{
    borrow::Cow,
    collections::HashMap,
    io::{stdin, stdout, Stdin, Stdout},
    rc::Rc,
};

use musdk_common::{
    incoming_message::IncomingMessage,
    outgoing_message::{FatalError, FunctionResult, Log, LogLevel, OutgoingMessage},
    Request, Response,
};

use crate::error::{Error, Result};

pub type MuFunction = Rc<dyn for<'a> Fn(&'a mut MuContext, &'a Request) -> Response<'static>>;

pub struct MuContext {
    stdin: Stdin,
    stdout: Stdout,

    functions: HashMap<String, MuFunction>,
}

impl MuContext {
    pub fn run<CF: ContextFactory>() {
        let mut context = CF::create_context();
        context.read_and_execute_function();
    }

    pub fn new(functions: HashMap<String, MuFunction>) -> Self {
        Self {
            stdin: stdin(),
            stdout: stdout(),
            functions,
        }
    }

    fn read_and_execute_function(&mut self) {
        fn helper(ctx: &mut MuContext) -> Result<()> {
            let message = ctx.read_message()?;
            let IncomingMessage::ExecuteFunction(execute_function) = message;
            //  else {
            //      return Err(Error::UnexpectedFirstMessageKind)
            // };
            let function = ctx
                .functions
                .get(execute_function.function.as_ref())
                .ok_or_else(|| Error::UnknownFunction(execute_function.function.into_owned()))?
                .clone();

            let response = (*function)(ctx, &execute_function.request);
            let message = OutgoingMessage::FunctionResult(FunctionResult { response });
            ctx.write_message(message)?;
            Ok(())
        }

        if let Err(f) = helper(self) {
            self.die(f);
        }
    }

    pub fn log(&mut self, message: &str, level: LogLevel) -> Result<()> {
        // TODO: set log level, check against given level, skip if necessary
        // TODO: make macros so the message doesn't have to be evaluated if its
        //       level is skipped
        let message = OutgoingMessage::Log(Log {
            body: Cow::Borrowed(message),
            level,
        });
        self.write_message(message)
    }

    fn die(&mut self, error: Error) -> ! {
        let error_description = error.to_string();
        let write_result = self.write_message(OutgoingMessage::FatalError(FatalError {
            error: Cow::Borrowed(&error_description),
        }));

        let mut panic_description = error_description;
        if let Err(f) = write_result {
            panic_description.push_str("; additionally, failed to write fatal error due to ");
            panic_description.push_str(&f.to_string());
        }
        panic!("{panic_description}");
    }

    fn read_message(&mut self) -> Result<IncomingMessage<'static>> {
        IncomingMessage::read(&mut self.stdin).map_err(Error::CannotDeserializeIncomingMessage)
    }

    fn write_message(&mut self, message: OutgoingMessage<'_>) -> Result<()> {
        message
            .write(&mut self.stdout)
            .map_err(Error::CannotSerializeOutgoingMessage)
    }
}

pub trait ContextFactory {
    fn create_context() -> MuContext;
}
