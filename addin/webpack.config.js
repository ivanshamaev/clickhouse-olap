const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");
const CopyWebpackPlugin = require("copy-webpack-plugin");
const fs = require("fs");
const os = require("os");

const certDir = path.join(os.homedir(), ".office-addin-dev-certs");

function loadCerts() {
  try {
    return {
      key: fs.readFileSync(path.join(certDir, "localhost.key")),
      cert: fs.readFileSync(path.join(certDir, "localhost.crt")),
      ca: fs.readFileSync(path.join(certDir, "ca.crt")),
    };
  } catch {
    // Certs not installed yet — run `npm run certs` to install
    return true;
  }
}

module.exports = (env, argv) => {
  const isDev = argv.mode === "development";

  return {
    entry: {
      taskpane: "./src/taskpane/taskpane.ts",
    },
    output: {
      path: path.resolve(__dirname, "dist"),
      filename: "[name].bundle.js",
      clean: true,
    },
    resolve: {
      extensions: [".ts", ".js"],
    },
    module: {
      rules: [
        {
          test: /\.ts$/,
          use: "ts-loader",
          exclude: /node_modules/,
        },
        {
          test: /\.css$/,
          use: ["style-loader", "css-loader"],
        },
      ],
    },
    plugins: [
      new HtmlWebpackPlugin({
        filename: "taskpane.html",
        template: "./src/taskpane/taskpane.html",
        chunks: ["taskpane"],
      }),
      new CopyWebpackPlugin({
        patterns: [
          { from: "assets", to: "assets" },
          { from: "manifest.xml", to: "manifest.xml" },
        ],
      }),
    ],
    devtool: isDev ? "source-map" : false,
    devServer: {
      port: 3000,
      server: {
        type: "https",
        options: loadCerts(),
      },
      headers: {
        "Access-Control-Allow-Origin": "*",
      },
      hot: true,
      static: {
        directory: path.join(__dirname, "dist"),
      },
    },
  };
};
