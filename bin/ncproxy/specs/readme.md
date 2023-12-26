The `Nanocl Controller Proxy` is an `HTTP REST API`.<br />
It is the `API` the `Nanocl Daemon` uses, to create / update and delete `ncproxy.io/rule`

## OpenAPI Specification
This API is documented in **OpenAPI format** using [Utoipa](https://github.com/juhaku/utoipa)<br />
The specification is generated automatically when running in development only.<br />
When releasing a version, the generated file is transfered to our [Documentation](https://github.com/next-hat/documentation).


## Cross-Origin Resource Sharing
This API features Cross-Origin Resource Sharing (CORS) implemented in compliance with  [W3C spec](https://www.w3.org/TR/cors/).<br />
And that allows cross-domain communication from the browser.<br />
All responses have a wildcard same-origin which makes them completely public and accessible to everyone, including any code on any site.
