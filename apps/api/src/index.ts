import { serve } from "@hono/node-server";
import { createServerApp } from "./app";

const port = Number(process.env.PORT) || 4000;
const app = createServerApp();

console.log(`[API] Ember POS API server starting on port ${port}...`);

serve(
  {
    fetch: app.fetch,
    port,
  },
  (info) => {
    console.log(`[API] Server listening at http://localhost:${info.port}`);
  },
);
