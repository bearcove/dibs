//! Filter argument specification and parsing.

/// Specification of what kind of argument is valid at a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgSpec {
    /// Accepts a variable reference (starting with $) or a literal value.
    VariableOrLiteral,
    /// Accepts only a variable reference.
    Variable,
    /// Accepts only a literal value.
    Literal,
}

/// Parsed filter argument.
#[derive(Debug, Clone)]
pub enum FilterArg {
    /// A variable reference like $handle (stored without the $).
    Variable(String),
    /// A literal value.
    Literal(String),
}

/// Specification for a filter function.
pub struct FunctionSpec {
    pub name: &'static str,
    pub args: &'static [ArgSpec],
}

impl FunctionSpec {
    /// Validate and parse arguments according to this spec.
    ///
    /// Returns rich errors if argument count doesn't match or argument types don't match spec.
    pub fn parse_args(&self, args: &[Meta<String>]) -> Result<Vec<FilterArg>, QueryGenError> {
        // Check argument count
        if args.len() != self.args.len() {
            return Err(QueryGenError::InvalidFilterArgs {
                filter: self.name.to_string(),
                reason: format!(
                    "expects {} argument(s), got {}",
                    self.args.len(),
                    args.len()
                ),
            });
        }

        // Parse each argument according to its spec
        let mut parsed = Vec::with_capacity(args.len());
        for (i, (arg_meta, spec)) in args.iter().zip(self.args.iter()).enumerate() {
            let arg_str = arg_meta.as_str();
            let is_var = arg_str.starts_with('$');

            let filter_arg = match spec {
                ArgSpec::VariableOrLiteral => {
                    if is_var {
                        FilterArg::Variable(arg_str[1..].to_string())
                    } else {
                        FilterArg::Literal(arg_str.to_string())
                    }
                }
                ArgSpec::Variable => {
                    if !is_var {
                        return Err(QueryGenError::InvalidFilterArgs {
                            filter: self.name.to_string(),
                            reason: format!(
                                "argument {} must be a variable reference (starting with $), got literal",
                                i
                            ),
                        });
                    }
                    FilterArg::Variable(arg_str[1..].to_string())
                }
                ArgSpec::Literal => {
                    if is_var {
                        return Err(QueryGenError::InvalidFilterArgs {
                            filter: self.name.to_string(),
                            reason: format!(
                                "argument {} must be a literal value, got variable reference",
                                i
                            ),
                        });
                    }
                    FilterArg::Literal(arg_str.to_string())
                }
            };

            parsed.push(filter_arg);
        }

        Ok(parsed)
    }
}

// Define function specs for all filter operations
pub const EQ_SPEC: FunctionSpec = FunctionSpec {
    name: "eq",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const NE_SPEC: FunctionSpec = FunctionSpec {
    name: "ne",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const LT_SPEC: FunctionSpec = FunctionSpec {
    name: "lt",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const LTE_SPEC: FunctionSpec = FunctionSpec {
    name: "lte",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const GT_SPEC: FunctionSpec = FunctionSpec {
    name: "gt",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const GTE_SPEC: FunctionSpec = FunctionSpec {
    name: "gte",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const LIKE_SPEC: FunctionSpec = FunctionSpec {
    name: "like",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const ILIKE_SPEC: FunctionSpec = FunctionSpec {
    name: "ilike",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const IN_SPEC: FunctionSpec = FunctionSpec {
    name: "in",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const JSON_GET_SPEC: FunctionSpec = FunctionSpec {
    name: "json-get",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const JSON_GET_TEXT_SPEC: FunctionSpec = FunctionSpec {
    name: "json-get-text",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const CONTAINS_SPEC: FunctionSpec = FunctionSpec {
    name: "contains",
    args: &[ArgSpec::VariableOrLiteral],
};

pub const KEY_EXISTS_SPEC: FunctionSpec = FunctionSpec {
    name: "key-exists",
    args: &[ArgSpec::VariableOrLiteral],
};
