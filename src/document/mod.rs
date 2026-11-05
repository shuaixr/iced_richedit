mod error;
mod mark;
mod model;
mod position;
mod schema;
mod selection;
mod text_block;
mod transaction;
mod view;

pub use error::DocumentError;
pub use mark::Mark;
pub use model::Document;
pub use position::Position;
pub use selection::TextSelection;
pub use transaction::Transaction;

pub(crate) use text_block::TextBlockKind;
pub(crate) use transaction::{DocumentChange, TransactionResult};
pub(crate) use view::{ParagraphView, TextPosition};
