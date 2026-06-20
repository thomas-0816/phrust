use crate::TokenName;

pub(crate) fn keyword_or_magic_token(bytes: &[u8]) -> Option<TokenName> {
    const KEYWORDS: &[(&[u8], TokenName)] = &[
        (b"abstract", TokenName::Abstract),
        (b"and", TokenName::LogicalAnd),
        (b"array", TokenName::Array),
        (b"as", TokenName::As),
        (b"break", TokenName::Break),
        (b"callable", TokenName::Callable),
        (b"case", TokenName::Case),
        (b"catch", TokenName::Catch),
        (b"class", TokenName::Class),
        (b"clone", TokenName::Clone),
        (b"const", TokenName::Const),
        (b"continue", TokenName::Continue),
        (b"declare", TokenName::Declare),
        (b"default", TokenName::Default),
        (b"do", TokenName::Do),
        (b"echo", TokenName::Echo),
        (b"else", TokenName::Else),
        (b"elseif", TokenName::ElseIf),
        (b"empty", TokenName::Empty),
        (b"enddeclare", TokenName::EndDeclare),
        (b"endfor", TokenName::EndFor),
        (b"endforeach", TokenName::EndForeach),
        (b"endif", TokenName::EndIf),
        (b"endswitch", TokenName::EndSwitch),
        (b"endwhile", TokenName::EndWhile),
        (b"enum", TokenName::Enum),
        (b"eval", TokenName::Eval),
        (b"exit", TokenName::Exit),
        (b"extends", TokenName::Extends),
        (b"final", TokenName::Final),
        (b"finally", TokenName::Finally),
        (b"fn", TokenName::Fn),
        (b"for", TokenName::For),
        (b"foreach", TokenName::Foreach),
        (b"function", TokenName::Function),
        (b"global", TokenName::Global),
        (b"goto", TokenName::Goto),
        (b"if", TokenName::If),
        (b"implements", TokenName::Implements),
        (b"include", TokenName::Include),
        (b"include_once", TokenName::IncludeOnce),
        (b"instanceof", TokenName::Instanceof),
        (b"insteadof", TokenName::InsteadOf),
        (b"interface", TokenName::Interface),
        (b"isset", TokenName::Isset),
        (b"list", TokenName::List),
        (b"match", TokenName::Match),
        (b"namespace", TokenName::Namespace),
        (b"new", TokenName::New),
        (b"or", TokenName::LogicalOr),
        (b"print", TokenName::Print),
        (b"private", TokenName::Private),
        (b"protected", TokenName::Protected),
        (b"public", TokenName::Public),
        (b"readonly", TokenName::Readonly),
        (b"require", TokenName::Require),
        (b"require_once", TokenName::RequireOnce),
        (b"return", TokenName::Return),
        (b"static", TokenName::Static),
        (b"switch", TokenName::Switch),
        (b"throw", TokenName::Throw),
        (b"trait", TokenName::Trait),
        (b"try", TokenName::Try),
        (b"unset", TokenName::Unset),
        (b"use", TokenName::Use),
        (b"var", TokenName::Var),
        (b"while", TokenName::While),
        (b"xor", TokenName::LogicalXor),
        (b"yield", TokenName::Yield),
        (b"__halt_compiler", TokenName::HaltCompiler),
    ];

    for (keyword, token) in KEYWORDS {
        if eq_ignore_ascii_case(bytes, keyword) {
            return Some(*token);
        }
    }

    let token = if eq_ignore_ascii_case(bytes, b"__LINE__") {
        TokenName::Line
    } else if eq_ignore_ascii_case(bytes, b"__FILE__") {
        TokenName::File
    } else if eq_ignore_ascii_case(bytes, b"__DIR__") {
        TokenName::Dir
    } else if eq_ignore_ascii_case(bytes, b"__CLASS__") {
        TokenName::ClassC
    } else if eq_ignore_ascii_case(bytes, b"__TRAIT__") {
        TokenName::TraitC
    } else if eq_ignore_ascii_case(bytes, b"__METHOD__") {
        TokenName::MethodC
    } else if eq_ignore_ascii_case(bytes, b"__FUNCTION__") {
        TokenName::FuncC
    } else if eq_ignore_ascii_case(bytes, b"__NAMESPACE__") {
        TokenName::NamespaceC
    } else if eq_ignore_ascii_case(bytes, b"__PROPERTY__") {
        TokenName::PropertyC
    } else {
        return None;
    };

    Some(token)
}

pub(crate) fn eq_ignore_ascii_case(left: &[u8], right: &[u8]) -> bool {
    left.eq_ignore_ascii_case(right)
}
