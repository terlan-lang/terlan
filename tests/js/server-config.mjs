// Isolated validation checkouts can run the complete matrix concurrently.
export const port = Number(process.env.TERLAN_JS_TEST_PORT ?? 0);
if (!Number.isInteger(port) || port < 0 || port > 65535) {
  throw new Error('TERLAN_JS_TEST_PORT must be an integer from 0 to 65535');
}
