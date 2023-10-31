pub struct EmptyObject;

impl<'__s> utoipa::ToSchema<'__s> for EmptyObject {
  fn schema() -> (
    &'__s str,
    utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
  ) {
    (
      "EmptyObject",
      utoipa::openapi::ObjectBuilder::new()
        .nullable(true)
        .title(Some("EmptyObject"))
        .description(Some("EmptyObject"))
        .schema_type(utoipa::openapi::schema::SchemaType::Object)
        .build()
        .into(),
    )
  }
}

pub struct PortMap;

impl<'__s> utoipa::ToSchema<'__s> for PortMap {
  fn schema() -> (
    &'__s str,
    utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
  ) {
    (
      "PortMap",
      utoipa::openapi::ObjectBuilder::new()
        .nullable(true)
        .title(Some("PortMap"))
        .description(Some("PortMap"))
        .schema_type(utoipa::openapi::schema::SchemaType::Object)
        .property(
          "<port/tcp|udp>",
          utoipa::openapi::ArrayBuilder::new()
            .items(
              utoipa::openapi::ObjectBuilder::new()
                .property(
                  "HostPort",
                  utoipa::openapi::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::SchemaType::String)
                    .build(),
                )
                .property(
                  "HostIp",
                  utoipa::openapi::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::SchemaType::String)
                    .build(),
                )
                .build(),
            )
            .build(),
        )
        .into(),
    )
  }
}

pub fn resolve_ref(
  components: &openapi::Components,
  ref_or_schema: &openapi::RefOr<openapi::Schema>,
  currently_parsed_refs: &mut Vec<String>,
) -> openapi::RefOr<openapi::Schema> {
  match ref_or_schema {
    openapi::RefOr::Ref(component_ref) => {
      let schema_key = component_ref.ref_location.split('/').last().unwrap();
      let component_schema = components.schemas.get_key_value(schema_key);
      if component_schema.is_none() {
        panic!("Schema {schema_key} doesn't exists")
      }
      let component_schema = component_schema.unwrap().1;
      if (currently_parsed_refs.contains(&schema_key.to_owned())) {
        panic!("Circular schema reference");
      }
      currently_parsed_refs.push(schema_key.to_owned());
      let result =
        resolve_refs(components, component_schema, currently_parsed_refs);
      currently_parsed_refs.pop();
      result
    }
    openapi::RefOr::T(..) => {
      resolve_refs(components, ref_or_schema, currently_parsed_refs)
    }
  }
}

pub fn map_resolve_ref(
  items: Vec<RefOr<Schema>>,
  components: &openapi::Components,
  currently_parsed_refs: &mut Vec<String>,
) -> Vec<RefOr<Schema>> {
  items
    .clone()
    .into_iter()
    .map(|ref_or_schema| {
      resolve_ref(components, &ref_or_schema, currently_parsed_refs)
    })
    .collect()
}

pub fn resolve_refs(
  components: &openapi::Components,
  ref_or_schema: &openapi::RefOr<openapi::Schema>,
  currently_parsed_refs: &mut Vec<String>,
) -> openapi::RefOr<openapi::Schema> {
  match ref_or_schema {
    openapi::RefOr::Ref(..) => {
      resolve_ref(components, ref_or_schema, currently_parsed_refs)
    }
    openapi::RefOr::T(schema) => match schema {
      openapi::Schema::AllOf(all_of) => {
        let mut result = all_of.clone();
        result.items =
          map_resolve_ref(result.items, components, currently_parsed_refs);

        openapi::RefOr::<openapi::Schema>::T(openapi::Schema::AllOf(result))
      }
      openapi::Schema::AnyOf(any_of) => {
        let mut result = any_of.clone();
        result.items =
          map_resolve_ref(result.items, components, currently_parsed_refs);

        openapi::RefOr::<openapi::Schema>::T(openapi::Schema::AnyOf(result))
      }
      openapi::Schema::Array(array) => {
        let mut result = array.clone();
        result.items = Box::new(resolve_ref(
          components,
          &array.items,
          currently_parsed_refs,
        ));

        openapi::RefOr::<openapi::Schema>::T(openapi::Schema::Array(result))
      }
      openapi::Schema::Object(object) => {
        let mut result = object.clone();
        result.properties = indexmap::map::IndexMap::new();
        object
          .properties
          .clone()
          .into_iter()
          .for_each(|(key, property)| {
            result.properties.insert(
              key,
              resolve_ref(components, &property, currently_parsed_refs),
            );
          });
        result.additional_properties =
          result.additional_properties.map(|additional_properties| {
            match *additional_properties {
              openapi::schema::AdditionalProperties::RefOr(ref_or_schema) => {
                Box::new(openapi::schema::AdditionalProperties::RefOr(
                  resolve_refs(
                    components,
                    &ref_or_schema,
                    currently_parsed_refs,
                  ),
                ))
              }
              openapi::schema::AdditionalProperties::FreeForm(_) => {
                additional_properties
              }
            }
          });
        openapi::RefOr::<openapi::Schema>::T(openapi::Schema::Object(result))
      }
      openapi::Schema::OneOf(one_of) => {
        let mut result = one_of.clone();
        result.items =
          map_resolve_ref(result.items, components, currently_parsed_refs);

        openapi::RefOr::<openapi::Schema>::T(openapi::Schema::OneOf(result))
      }
      &_ => todo!(),
    },
  }
}

// Generate JSON cargo config schema
pub fn generate_cargo_config_schema() {
  let out_dir = std::env::current_dir().unwrap();
  let components = ApiDoc::openapi().components.unwrap();
  let _resource_ref = &openapi::RefOr::<openapi::Schema>::Ref(
    openapi::Ref::new("#/components/schemas/CertManagerIssuer".to_owned()),
  );
  let resource_schema =
    resolve_refs(&components, _resource_ref, &mut Vec::new());
  let resource_schema_str = serde_json::json!({
    "Schema": resource_schema
  })
  .to_string();

  // schema.description = Some(
  //   "CertManagerIssuer Generate, contain CargoConfig object".to_owned(),
  // );
  // schema.title = Some("Certificates generation container".to_owned());

  // let resource_schema = ["{\"Schema\":{\"description\":\"CertManagerIssuer Generate, contain CargoConfig object\",\"properties\":{\"Generate\": ".to_owned(), cargo_config_schema_str, "},\"required\":[\"Generate\"],\"title\":\"Certificates generation container\",\"type\":\"object\"}}".to_owned()].join("");
  fs::write(out_dir.join(RESOURCE_SCHEMA_FILENAME), resource_schema_str)
    .unwrap();
}
