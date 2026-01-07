pub mod generator;
mod parser;

pub use parser::{
    Declarations, Field, FunctionDecl, Parameter, StructDecl, Type, VariableDecl,
    parse_declarations,
};
