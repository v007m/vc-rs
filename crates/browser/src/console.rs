//! The spec reporter's destination in the playground: console text to the page's stdout, and no
//! report files, since the page has nowhere to keep them.

use std::path::PathBuf;
use std::sync::Arc;

use vc_adapter_reporters::Destination;
use vc_interfaces::ReportError;

use crate::{Sink, Stream};

/// Prints to a [`Sink`]'s stdout and refuses every report file.
#[derive(Clone, Debug)]
pub struct SinkDestination {
    sink: Arc<dyn Sink>,
}

impl SinkDestination {
    pub fn new(sink: Arc<dyn Sink>) -> Self {
        Self { sink }
    }
}

impl Destination for SinkDestination {
    fn print(&self, text: &str) -> Result<(), ReportError> {
        self.sink.print(Stream::Out, text);
        Ok(())
    }

    fn save(&self, name: &str, _contents: &[u8]) -> Result<PathBuf, ReportError> {
        let reason = "the playground writes no report files".to_owned();
        Err(ReportError::Write { location: name.to_owned(), reason })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vc_adapter_reporters::Destination;
    use vc_interfaces::ReportError;

    use super::SinkDestination;
    use crate::Stream;
    use crate::testing::Captured;

    #[test]
    fn console_text_goes_to_stdout_and_files_are_refused() {
        let (sink, captured) = Captured::sink();
        let destination = SinkDestination::new(Arc::new(sink));
        destination.print("✓ passes\n").unwrap();
        let refused = destination.save("ctrf.json", b"{}").unwrap_err();
        assert!(matches!(&refused, ReportError::Write { location, .. } if location == "ctrf.json"));
        assert_eq!(captured.take(), [(Stream::Out, "✓ passes\n".to_owned())]);
    }
}
