import * as Todo from "../build/js/modules/terlan/angular/Todo.js";

const initial = Todo.initial_state();
const summary = Todo.summary(initial.title, "empty");
const created = Todo.create_item(initial.next_id, "Review");
const toggled = Todo.toggle_item(created);
const edited = Todo.edit_item(created);
const checks = [
  Todo.app_module() === "terlanTodo",
  Todo.controller_name() === "TodoController",
  initial.title === "Terlan Angular Todo",
  summary === `<section data-state="empty">
  <h1>Terlan&#32;Angular&#32;Todo</h1>
</section>
`,
  Todo.can_create("Review") === true && Todo.can_create("") === false,
  created.id === 1 && created.title === "todo:Review" && created.completed === false,
  Todo.next_id(created.id) === 2,
  Todo.clear_draft() === "",
  toggled.completed === true && created.completed === false,
  edited.title === "todo:Review edited",
  Todo.keep_item(created, 2) === true,
  Todo.keep_item(created, 1) === false,
  Todo.visible(created, "active") === true,
  Todo.visible(toggled, "completed") === true,
  Todo.visible(created, "completed") === false,
  Todo.select_filter("active") === "active",
  Todo.list_state(0) === "empty",
  Todo.list_state(2) === "list",
  Todo.render_row(created) === "todo:Review:active",
  Todo.render_row(toggled) === "todo:Review:done",
];

if (!checks.every(Boolean)) {
  throw new Error("Terlan-owned todo app behavior failed");
}
