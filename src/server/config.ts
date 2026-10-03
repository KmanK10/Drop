export type AppConfig = {
  dataDir: string;
  publicUrl: string;
  cookieSecure: boolean;
  trustProxy: boolean;
  port: number;
  host: string;
  setupSecret: string | undefined;
};

export function readConfig(env: NodeJS.ProcessEnv): AppConfig {
  const publicUrl = (env.PUBLIC_URL ?? "").replace(/\/+$/, "");
  const cookieSecure =
    env.COOKIE_SECURE === undefined || env.COOKIE_SECURE === ""
      ? publicUrl.startsWith("https://")
      : env.COOKIE_SECURE === "true";
  const trustProxy = (env.TRUST_PROXY ?? "true") === "true";
  const port = Number(env.PORT ?? 8080);
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error("PORT is not a valid port.");
  }
  return {
    dataDir: env.DATA_DIR && env.DATA_DIR.length > 0 ? env.DATA_DIR : "./data",
    publicUrl,
    cookieSecure,
    trustProxy,
    port,
    host: env.HOST && env.HOST.length > 0 ? env.HOST : "0.0.0.0",
    setupSecret: env.SETUP_SECRET,
  };
}

export function setupConfigError(userCount: number, secret: string | undefined): string | null {
  if (userCount > 0) return null;
  if (!secret || secret.length < 16) {
    return "SETUP_SECRET must be set to at least 16 characters before the first admin exists.";
  }
  return null;
}
