use std::sync::Arc;

use arrow::{
    array::{
        Array, ArrayData, ArrayRef, BooleanBuilder, Float64Builder, Int32Builder, Int64Builder,
        ListArray, StringBuilder, StructArray, TimestampMillisecondBuilder, new_null_array,
    },
    buffer::Buffer,
    compute::concat,
    datatypes::{DataType, Field, SchemaRef, TimeUnit},
    error::{ArrowError, Result},
    record_batch::RecordBatch,
};
use serde_json::Value;

/// Converts a JSON object into a RecordBatch with schema
pub fn convert_json(data: &Value, schema: &SchemaRef) -> Result<RecordBatch> {
    let obj = data.as_object().ok_or_else(|| {
        ArrowError::ParseError("Expected JSON object at the top level".to_string())
    })?;

    let arrays = schema
        .fields()
        .iter()
        .map(|f| build_array(obj.get(f.name()), f))
        .collect::<Result<Vec<_>>>()?;

    RecordBatch::try_new(schema.clone(), arrays)
}

/// Builds an arrow Array from a JSON value
fn build_array(value: Option<&Value>, field: &Field) -> Result<ArrayRef> {
    match value {
        None => {
            if !field.is_nullable() {
                return Err(ArrowError::ParseError(format!(
                    "Missing required field '{}'",
                    field.name()
                )));
            }

            match field.data_type() {
                DataType::List(child_field) => {
                    let data = arrow::array::ArrayData::builder(field.data_type().clone())
                        .len(1)
                        .add_buffer(Buffer::from_slice_ref([0i32, 0i32]))
                        .add_child_data(new_null_array(child_field.data_type(), 0).to_data())
                        .null_bit_buffer(Some(Buffer::from_slice_ref([0u8])))
                        .build()?;
                    Ok(Arc::new(arrow::array::ListArray::from(data)))
                }
                DataType::Struct(child_fields) => {
                    let children: Vec<(Arc<Field>, ArrayRef)> = child_fields
                        .iter()
                        .map(|child| (child.clone(), new_null_array(child.data_type(), 1)))
                        .collect();
                    Ok(Arc::new(StructArray::from(children)))
                }
                _ => Ok(new_null_array(field.data_type(), 1)),
            }
        }
        Some(v) => match field.data_type() {
            DataType::Int32 => {
                let mut builder = Int32Builder::new();
                if let Some(n) = v.as_i64() {
                    if n < i32::MIN as i64 || n > i32::MAX as i64 {
                        if field.is_nullable() {
                            eprintln!(
                                "Warning: integer {} out of range for field '{}'; inserting null",
                                n,
                                field.name()
                            );
                            builder.append_null();
                        } else {
                            return Err(ArrowError::ParseError(format!(
                                "Integer {} out of range for field '{}'",
                                n,
                                field.name()
                            )));
                        }
                    } else {
                        builder.append_value(n as i32);
                    }
                } else if field.is_nullable() {
                    eprintln!(
                        "Warning: expected integer for field '{}'; inserting null",
                        field.name()
                    );
                    builder.append_null();
                } else {
                    return Err(ArrowError::ParseError(format!(
                        "Expected integer for field '{}'",
                        field.name()
                    )));
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Int64 => {
                let mut builder = Int64Builder::new();
                if let Some(n) = v.as_i64() {
                    builder.append_value(n);
                } else if field.is_nullable() {
                    eprintln!(
                        "Warning: expected integer for field '{}'; inserting null",
                        field.name()
                    );
                    builder.append_null();
                } else {
                    return Err(ArrowError::ParseError(format!(
                        "Expected integer for field '{}'",
                        field.name()
                    )));
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Float64 => {
                let mut builder = Float64Builder::new();
                if let Some(f) = v.as_f64() {
                    builder.append_value(f);
                } else if let Some(n) = v.as_i64() {
                    builder.append_value(n as f64);
                } else if field.is_nullable() {
                    eprintln!(
                        "Warning: expected float for field '{}'; inserting null",
                        field.name()
                    );
                    builder.append_null();
                } else {
                    return Err(ArrowError::ParseError(format!(
                        "Expected float for field '{}'",
                        field.name()
                    )));
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Boolean => {
                let mut builder = BooleanBuilder::new();
                if let Some(b) = v.as_bool() {
                    builder.append_value(b);
                } else if field.is_nullable() {
                    eprintln!(
                        "Warning: expected boolean for field '{}'; inserting null",
                        field.name()
                    );
                    builder.append_null();
                } else {
                    return Err(ArrowError::ParseError(format!(
                        "Expected boolean for field '{}'",
                        field.name()
                    )));
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Utf8 | DataType::Binary => {
                let mut builder = StringBuilder::new();
                if let Some(s) = v.as_str() {
                    builder.append_value(s);
                } else if v.is_null() {
                    if field.is_nullable() {
                        eprintln!(
                            "Warning: expected string for field '{}'; inserting null",
                            field.name()
                        );
                        builder.append_null();
                    } else {
                        return Err(ArrowError::ParseError(format!(
                            "Expected string for field '{}'",
                            field.name()
                        )));
                    }
                } else {
                    builder.append_value(v.to_string());
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Struct(children) => {
                let obj = v.as_object().ok_or_else(|| {
                    ArrowError::ParseError(format!(
                        "Expected JSON object for struct field '{}'",
                        field.name()
                    ))
                })?;

                let child_array = children
                    .iter()
                    .map(|child| build_array(obj.get(child.name()), child))
                    .collect::<Result<Vec<_>>>()?;

                let data = children
                    .iter()
                    .cloned()
                    .zip(child_array)
                    .collect::<Vec<(Arc<Field>, ArrayRef)>>();

                Ok(Arc::new(StructArray::from(data)))
            }
            DataType::List(child_field) => {
                let json_array = v.as_array().ok_or_else(|| {
                    ArrowError::ParseError(format!(
                        "Expected JSON array for list field '{}'",
                        field.name()
                    ))
                })?;

                if json_array.is_empty() {
                    let data = ArrayData::builder(field.data_type().clone())
                        .len(1)
                        .add_buffer(Buffer::from_slice_ref([0i32, 0i32]))
                        .add_child_data(new_null_array(child_field.data_type(), 0).to_data())
                        .build()?;

                    return Ok(Arc::new(ListArray::from(data)));
                }

                let inner_arrays = json_array
                    .iter()
                    .map(|elem| build_array(Some(elem), child_field))
                    .collect::<Result<Vec<_>>>()?;

                let inner = concat(&inner_arrays.iter().map(|a| a.as_ref()).collect::<Vec<_>>())?;

                let data = ArrayData::builder(field.data_type().clone())
                    .len(1)
                    .add_buffer(Buffer::from_slice_ref([0i32, inner.len() as i32]))
                    .add_child_data(inner.to_data())
                    .build()?;

                Ok(Arc::new(ListArray::from(data)))
            }
            DataType::Timestamp(TimeUnit::Millisecond, tz) => {
                let mut builder = TimestampMillisecondBuilder::new();
                if let Some(ts) = v.as_i64() {
                    builder.append_value(ts);
                } else if let Some(s) = v.as_str() {
                    if let Ok(ts) = s.parse::<i64>() {
                        builder.append_value(ts);
                    } else if field.is_nullable() {
                        eprintln!(
                            "Warning: expected timestamp for field '{}'; inserting null",
                            field.name()
                        );
                        builder.append_null();
                    } else {
                        return Err(ArrowError::ParseError(format!(
                            "Expected timestamp for field '{}'",
                            field.name()
                        )));
                    }
                } else if field.is_nullable() {
                    eprintln!(
                        "Warning: expected timestamp for field '{}'; inserting null",
                        field.name()
                    );
                    builder.append_null();
                } else {
                    return Err(ArrowError::ParseError(format!(
                        "Expected timestamp for field '{}'",
                        field.name()
                    )));
                }
                Ok(Arc::new(builder.finish().with_timezone_opt(tz.clone())))
            }
            dt => Err(ArrowError::NotYetImplemented(format!(
                "Data type {:?} not supported for field '{}'",
                dt,
                field.name()
            ))),
        },
    }
}
