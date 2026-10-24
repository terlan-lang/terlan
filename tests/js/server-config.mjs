// Isolated validation checkouts can run the complete matrix concurrently.
export const port = Number(process.env.TERLAN_JS_TEST_PORT ?? 4197);
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error('TERLAN_JS_TEST_PORT must be an integer from 1 to 65535');
}
export const url = `http://127.0.0.1:${port}`;
