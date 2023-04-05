window.onload = function() {
  //<editor-fold desc="Changeable Configuration Block">

  // the following lines will be replaced by docker/configurator, when it runs in a docker-container
  window.ui = SwaggerUIBundle({
    url: "/explorer/swagger.json",
    dom_id: '#swagger-ui',
    deepLinking: true,
    operationsSorter: 'alpha',
    apisSorter: 'alpha',
    tagsSorter: 'alpha',
    presets: [
      SwaggerUIBundle.presets.apis,
      SwaggerUIStandalonePreset
    ],
    plugins: [
      SwaggerUIBundle.plugins.DownloadUrl
    ],
  });

  //</editor-fold>
};
