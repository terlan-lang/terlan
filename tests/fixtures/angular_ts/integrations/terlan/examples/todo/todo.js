import { angular } from "../../../../dist/index.js";
import * as Todo from "../../build/js/modules/terlan/angular/Todo.js";

const app = angular.module(Todo.app_module(), []);

app.directive("terlanCallback", () => ({
  restrict: "A",
  link(_scope, element) {
    element.dataset.terlanDirective = "mounted";
    const onProbe = () => {
      element.dataset.terlanCallback = "invoked";
    };
    element.addEventListener("terlan:probe", onProbe, { once: true });
  },
}));

app.model("todoModel", () => {
  const initial = Todo.initial_state();

  return {
    draft: initial.draft,
    filter: initial.filter,
    nextId: initial.next_id,
    items: [],
    state: Todo.list_state(0),
  };
});

app.controller(Todo.controller_name(), ["todoModel", function TodoController(todoModel) {
  const initial = Todo.initial_state();
  this.title = initial.title;
  this.model = todoModel;

  const refreshItem = (item) => {
    item.visible = Todo.visible(item, todoModel.filter);
    item.label = Todo.render_row(item);
  };

  this.add = (text) => {
    if (!Todo.can_create(text)) return;

    const item = Todo.create_item(todoModel.nextId, text);
    refreshItem(item);
    todoModel.items.push(item);
    todoModel.nextId = Todo.next_id(todoModel.nextId);
    todoModel.draft = Todo.clear_draft();
    todoModel.state = Todo.list_state(todoModel.items.length);
  };
  this.toggle = (item) => {
    Object.assign(item, Todo.toggle_item(item));
    refreshItem(item);
  };
  this.edit = (item) => {
    Object.assign(item, Todo.edit_item(item));
    refreshItem(item);
  };
  this.remove = (item) => {
    const index = todoModel.items.findIndex(
      (candidate) => !Todo.keep_item(candidate, item.id),
    );
    if (index >= 0) todoModel.items.splice(index, 1);
    todoModel.state = Todo.list_state(todoModel.items.length);
  };
  this.setFilter = (filter) => {
    todoModel.filter = Todo.select_filter(filter);
    for (const item of todoModel.items) refreshItem(item);
  };
}]);

angular.bootstrap(document.body, [app.name]);
