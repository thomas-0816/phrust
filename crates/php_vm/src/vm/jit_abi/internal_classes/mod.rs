use super::*;

mod date_time;
mod dom;
mod mysqli;
mod pdo;
mod phar;
mod simple_xml;
mod spl_iterators;
mod support;
mod xml_reader;
mod xml_writer;

use date_time::{
    construct_native_date_time, date_time_class_constant, date_time_instanceof,
    execute_native_date_time_instruction,
};
use mysqli::{construct_native_mysqli_class, execute_native_mysqli_instruction};
use pdo::pdo_mysql_class_constant;
pub(super) use pdo::pdo_mysql_deprecated_constant;
use phar::execute_native_phar_instruction;
pub(super) use simple_xml::{
    construct_native_simple_xml, execute_native_simple_xml_instruction, native_simple_xml_count,
    native_simple_xml_dimension, native_simple_xml_empty, native_simple_xml_entries,
    native_simple_xml_property, native_simple_xml_text,
};
use spl_iterators::spl_iterator_class_constant;
pub(super) use spl_iterators::{construct_native_spl_iterator, native_spl_iterator_entries};
use support::*;

pub(super) use dom::{
    construct_native_dom_class, execute_native_dom_instruction, native_dom_collection_entries,
};
use xml_reader::{
    construct_native_xml_reader, execute_native_xml_reader_instruction, xml_reader_class_constant,
};
use xml_writer::{
    construct_native_xml_writer, execute_native_xml_writer_builtin,
    execute_native_xml_writer_instruction,
};

pub(super) fn construct_native_internal_class(
    context: &mut NativeExecutionContext<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    construct_native_dom_class(context, class_name, arguments)
        .or_else(|| construct_native_date_time(context, class_name, arguments))
        .or_else(|| construct_native_mysqli_class(context, class_name, arguments))
        .or_else(|| construct_native_simple_xml(context, class_name, arguments))
        .or_else(|| construct_native_spl_iterator(context, class_name, arguments))
        .or_else(|| construct_native_xml_reader(context, class_name, arguments))
        .or_else(|| construct_native_xml_writer(context, class_name, arguments))
}

pub(super) fn execute_native_internal_class(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    execute_native_dom_instruction(context, instruction, arguments)
        .or_else(|| execute_native_date_time_instruction(context, instruction, arguments))
        .or_else(|| execute_native_mysqli_instruction(context, instruction, arguments))
        .or_else(|| execute_native_phar_instruction(context, instruction, arguments))
        .or_else(|| execute_native_simple_xml_instruction(context, instruction, arguments))
        .or_else(|| execute_native_xml_reader_instruction(context, instruction, arguments))
        .or_else(|| execute_native_xml_writer_instruction(context, instruction, arguments))
}

pub(super) fn execute_native_internal_builtin(
    context: &mut NativeExecutionContext<'_>,
    name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    execute_native_xml_writer_builtin(context, name, arguments)
}

pub(super) fn native_internal_class_constant(class_name: &str, constant: &str) -> Option<Value> {
    date_time_class_constant(class_name, constant)
        .or_else(|| pdo_mysql_class_constant(class_name, constant))
        .or_else(|| spl_iterator_class_constant(class_name, constant))
        .or_else(|| xml_reader_class_constant(class_name, constant))
}

pub(super) fn native_internal_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    date_time_instanceof(object_class, target_class)
}
