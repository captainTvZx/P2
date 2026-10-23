{
  "targets": [
    {
      "target_name": "p2overlayhost",
      "sources": [ "host.cc" ],
      "include_dirs": [ "<!(node -p \"require('node-addon-api').include_dir\")", "../shared" ],
      "defines": [ "NAPI_DISABLE_CPP_EXCEPTIONS" ],
      "conditions": [
        [ "OS=='win'", { "libraries": [] } ]
      ]
    }
  ]
}
