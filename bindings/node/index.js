const path = require('path');

let native;
try {
  native = require(path.join(__dirname, 'index.node'));
} catch (err) {
  throw new Error(
    'Unable to load the native TOON bindings. Did you run "npm run build" to compile them?\n' +
      err.message
  );
}

module.exports = native;
