{
  "targets": [
    {
      "target_name": "loopback",
      "sources": ["src/loopback.cc"],
      "defines": ["NAPI_VERSION=8", "UNICODE", "_UNICODE"],
      "conditions": [
        ["OS=='win'", {
          "libraries": ["-lole32", "-loleaut32", "-lavrt", "-lmmdevapi"],
          "msvs_settings": { "VCCLCompilerTool": { "ExceptionHandling": 1 } }
        }]
      ]
    }
  ]
}
