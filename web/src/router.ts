import { createMemoryHistory, createRouter } from "vue-router";

export const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: "/", name: "panel", component: () => import("./views/PanelView.vue") },
  ],
});
