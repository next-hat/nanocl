The `Nanocl Daemon` is an `HTTP REST API`.<br />
It is the `API` the `Nanocl Client` uses, so everything the `Nanocl Client` can do can be done with the `API`.

Most of the client's commands map directly to API endpoints e.g: `nanocl ps` is `GET /processes`.<br />
The notable exception is running `Cargo`, which consists of several `API` calls.


## OpenAPI Specification
This API is documented in **OpenAPI format** using [Utoipa](https://github.com/juhaku/utoipa)<br />
The specification is generated automatically when running in development only.<br />
When releasing a version, the generated file is transferred to our [Documentation](https://github.com/next-hat/documentation).


## Cross-Origin Resource Sharing
This API features Cross-Origin Resource Sharing (CORS) implemented in compliance with  [W3C spec](https://www.w3.org/TR/cors/).<br />
And that allows cross-domain communication from the browser.<br />
All responses have a wildcard same-origin which makes them completely public and accessible to everyone, including any code on any site.
