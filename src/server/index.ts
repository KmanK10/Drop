import { createApp } from "./app.ts";
import { readConfig, setupConfigError } from "./config.ts";
import { createLogger } from "./log.ts";

const config = readConfig(process.env);
const log = createLogger();
const drop = createApp({
  dataDir: config.dataDir,
  setupSecret: config.setupSecret,
  publicUrl: config.publicUrl,
  cookieSecure: config.cookieSecure,
  trustProxy: config.trustProxy,
  log,
});

const problem = setupConfigError(drop.userCount(), config.setupSecret);
if (problem) {
  log({ level: "error", msg: problem });
  await drop.close();
  process.exit(1);
}

const running = await drop.listen(config.port, config.host);
log({ level: "info", msg: "listening", route: `${config.host}:${running.port}` });

let shuttingDown = false;
async function shutdown(): Promise<void> {
  if (shuttingDown) return;
  shuttingDown = true;
  log({ level: "info", msg: "shutdown" });
  await drop.close();
  process.exit(0);
}

process.on("SIGTERM", () => {
  void shutdown();
});
process.on("SIGINT", () => {
  void shutdown();
});
