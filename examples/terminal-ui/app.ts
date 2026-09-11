import {
  authenticate,
  getWorkspace,
  getSetup,
  getSetupStatus,
  saveSetup,
  cancelSetup,
  getResult,
  getRows,
  cancelResult,
  type SetupStatus,
  type SetupSubmission,
} from "../../ui/sdk/client";

// An independent interface using only the public browser SDK.
const root = document.querySelector<HTMLElement>("main")!;
const feedback = document.querySelector<HTMLElement>("#feedback")!;
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "") {
  const item = document.createElement(tag);
  item.textContent = text;
  return item;
}
function error(reason: unknown) {
  feedback.textContent =
    reason instanceof Error ? reason.message : String(reason);
}
function button(label: string, action: () => Promise<void>) {
  const item = node("button", label);
  item.type = "button";
  item.onclick = async () => {
    item.disabled = true;
    feedback.textContent = "";
    try {
      await action();
    } catch (reason) {
      error(reason);
    } finally {
      item.disabled = false;
    }
  };
  return item;
}
function later(action: () => Promise<void>) {
  window.setTimeout(() => {
    void action().catch(error);
  }, 1000);
}
async function home() {
  const workspace = await getWorkspace();
  root.append(node("h1", "Your workspace"));
  for (const [title, entries, route] of [
    ["Connections to finish", workspace.setups, "setup"],
    ["Saved query results", workspace.results, "result"],
  ] as const) {
    root.append(node("h2", title));
    const list = node("ul");
    for (const entry of entries) {
      const link = node("a", `${entry.name} / ${entry.status}`);
      link.href = `/${route}/${entry.id}`;
      const row = node("li");
      row.append(link);
      list.append(row);
    }
    root.append(entries.length ? list : node("p", "No entries yet."));
  }
}
async function setup(id: string) {
  const draft = await getSetup(id);
  root.append(
    node("h1", draft.name),
    node(
      "p",
      `${draft.connection.database_type} / ${draft.connection.host}:${draft.connection.port} / ${draft.connection.database || draft.connection.service}`,
    ),
  );
  const details = node("details");
  details.append(
    node("summary", "Prepared connection settings"),
    node("pre", JSON.stringify(draft.connection, null, 2)),
  );
  root.append(details);
  const form = node("form"),
    fields = node("fieldset"),
    status = node("p");
  status.setAttribute("role", "status");
  const userLabel = node("label", "Username"),
    username = node("input");
  username.name = "username";
  username.value = draft.connection.username;
  username.autocomplete = "username";
  username.required = true;
  userLabel.append(username);
  const passwordLabel = node("label", "Password"),
    password = node("input");
  password.name = "password";
  password.type = "password";
  password.autocomplete = "current-password";
  passwordLabel.append(password);
  const modeLabel = node("label", "Password handling"),
    mode = node("select");
  const choices: [SetupSubmission["password_action"], string][] = [
    ...(draft.editing
      ? [["keep", "Keep saved password"] as ["keep", string]]
      : []),
    ["replace", "Enter a password"],
    ["clear", "Use an empty password"],
  ];
  for (const [value, label] of choices) {
    const option = node("option", label);
    option.value = value;
    mode.append(option);
  }
  const passwordMode = () => {
    password.disabled = mode.value !== "replace";
    if (password.disabled) password.value = "";
  };
  mode.onchange = passwordMode;
  passwordMode();
  modeLabel.append(mode);
  const submit = node("button", "Save connection");
  submit.type = "submit";
  const cancel = button("Cancel request", async () => {
    show(await cancelSetup(id));
  });
  fields.append(userLabel, modeLabel, passwordLabel, submit, cancel);
  form.append(fields);
  root.append(status, form);
  function show(value: SetupStatus) {
    status.textContent =
      value.status + (value.datasource_id ? ` / ${value.datasource_id}` : "");
    fields.disabled = value.status !== "waiting_for_user";
    if (value.error) feedback.textContent = value.error;
    if (value.status === "saving")
      later(async () => show(await getSetupStatus(id)));
  }
  form.onsubmit = async (event) => {
    event.preventDefault();
    fields.disabled = true;
    feedback.textContent = "";
    const action = choices.find(([key]) => key === mode.value)![0];
    const input: SetupSubmission = {
      name: draft.name,
      connection: {
        ...draft.connection,
        username: username.value,
        password: password.value,
      },
      password_action: action,
    };
    password.value = "";
    try {
      show(await saveSetup(id, input));
    } catch (reason) {
      fields.disabled = false;
      error(reason);
    }
  };
  show(draft);
}
async function result(id: string) {
  root.append(node("h1", "Query results"));
  const status = node("p"),
    sql = node("details"),
    output = node("section"),
    events = node("pre");
  status.setAttribute("role", "status");
  const cancel = button("Cancel query", async () => {
    await cancelResult(id);
  });
  root.append(status, cancel, sql, output, events);
  let selected = 0;
  let offsets = [0];
  async function refresh() {
    const meta = await getResult(id);
    const running = meta.status === "running" || meta.status === "queued";
    status.textContent = `${meta.datasource_name} / ${meta.status} / ${meta.duration_ms} ms`;
    cancel.hidden = !running;
    sql.replaceChildren(
      node("summary", "SQL statements"),
      node("pre", meta.statements.join("\n\n")),
    );
    events.textContent = meta.events
      .filter((event) => ["error", "skipped"].includes(event.event))
      .map((event) => JSON.stringify(event))
      .join("\n");
    output.replaceChildren();
    if (meta.tables.length) {
      const tabs = node("select");
      tabs.setAttribute("aria-label", "Result set");
      meta.tables.forEach((table, index) => {
        const option = node(
          "option",
          `Statement ${table.statement + 1} / Result ${table.result + 1} / ${table.rows} rows`,
        );
        option.value = String(index);
        tabs.append(option);
      });
      tabs.value = String(selected);
      const grid = node("div"),
        paging = node("fieldset");
      grid.className = "grid";
      async function rows() {
        paging.disabled = true;
        tabs.disabled = true;
        const table = meta.tables[selected];
        const offset = offsets[offsets.length - 1];
        try {
          const page = await getRows(
            id,
            table.statement,
            table.result,
            offset,
            100,
          );
          const html = node("table"),
            head = node("tr");
          table.columns.forEach((column) =>
            head.append(node("th", column.name)),
          );
          const thead = node("thead");
          thead.append(head);
          html.append(thead);
          for (const values of page.rows) {
            const row = node("tr");
            values.forEach((value) =>
              row.append(
                node(
                  "td",
                  value === null
                    ? "NULL"
                    : typeof value === "string"
                      ? value
                      : JSON.stringify(value),
                ),
              ),
            );
            html.append(row);
          }
          grid.replaceChildren(html);
          if (table.affected_rows !== null)
            grid.prepend(node("p", `${table.affected_rows} row(s) affected.`));
          const previous = button("Previous", async () => {
            offsets.pop();
            await rows();
          });
          const next = button("Next", async () => {
            offsets.push(page.next_offset);
            await rows();
          });
          previous.disabled = offsets.length === 1;
          next.disabled = page.next_offset >= page.total_rows;
          paging.replaceChildren(
            node(
              "span",
              `${page.rows.length ? offset + 1 : 0}–${page.next_offset} / ${page.total_rows} `,
            ),
            previous,
            next,
          );
        } finally {
          paging.disabled = false;
          tabs.disabled = false;
        }
      }
      tabs.onchange = () => {
        selected = Number(tabs.value);
        offsets = [0];
        void rows().catch(error);
      };
      output.append(tabs, grid, paging);
      await rows();
    }
    if (running) later(refresh);
  }
  await refresh();
}
async function start() {
  await authenticate();
  const [, route, id] = location.pathname.split("/");
  if (route === "setup" && id) await setup(id);
  else if (route === "result" && id) await result(id);
  else await home();
}
void start().catch(error);
