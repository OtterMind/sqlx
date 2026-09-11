import { api } from "./api";
import { pollLater } from "./navigation";
import {
  button,
  element,
  field,
  heading,
  message,
  selectField,
  statusBadge,
} from "./components";

import type { SetupView as Setup } from "../sdk/types";

export async function setupPage(
  root: HTMLElement,
  id: string,
  onStatusChange: (status: string) => void,
  signal: AbortSignal,
): Promise<void> {
  const setup = await api<Setup>(`/setups/${id}`, undefined, signal);
  signal.throwIfAborted();
  root.className = "setup-page";
  root.replaceChildren(
    heading(setup.editing ? "Edit connection" : "Connect datasource"),
  );
  const card = element("section", "card setup-card");
  const form = element("form");
  const intro = element("div", "card-heading");
  const status = element("div");
  status.append(statusBadge(setup.status));
  const identity = element("div", "connection-identity");
  identity.append(element("h2", "", setup.name));
  const context = element("p", "connection-context");
  identity.append(context);
  intro.append(identity, status);
  const fields = element("fieldset");
  const grid = element("div", "field-grid");
  const name = field("Connection name", "name", setup.name);
  const kind = selectField(
    "Database type",
    "database_type",
    [
      ["mysql", "MySQL"],
      ["postgresql", "PostgreSQL"],
      ["oracle", "Oracle"],
      ["sqlserver", "SQL Server"],
    ],
    setup.connection.database_type,
  );
  const host = field("Host", "host", setup.connection.host);
  const port = field("Port", "port", String(setup.connection.port), "number");
  port.input.min = "1";
  port.input.max = "65535";
  const database = field("Database", "database", setup.connection.database);
  const service = field("Service name", "service", setup.connection.service);
  const tls = selectField(
    "Connection security",
    "tls",
    [
      ["verify-full", "TLS · verify certificate"],
      ["disable", "Unencrypted connection"],
    ],
    setup.connection.tls,
  );
  const username = field("Username", "username", setup.connection.username);
  username.input.autocomplete = "username";
  const password = field("Password", "password", "", "password");
  password.input.autocomplete = "current-password";
  signal.addEventListener(
    "abort",
    () => {
      password.input.value = "";
    },
    { once: true },
  );
  const passwordAction = selectField(
    "Password handling",
    "password_action",
    setup.editing
      ? [
          ["keep", "Keep saved password"],
          ["replace", "Enter a new password"],
          ["clear", "Use an empty password"],
        ]
      : [
          ["replace", "Enter a password"],
          ["clear", "Use an empty password"],
        ],
    setup.editing ? "keep" : "replace",
  );
  name.input.required = true;
  host.input.required = true;
  port.input.required = true;
  username.input.required = true;
  grid.append(
    name.wrapper,
    kind.wrapper,
    host.wrapper,
    port.wrapper,
    database.wrapper,
    service.wrapper,
    tls.wrapper,
  );
  const credentials = element("div", "credentials");
  credentials.append(
    username.wrapper,
    passwordAction.wrapper,
    password.wrapper,
  );
  const details = element("details", "connection-details");
  details.append(element("summary", "", "Connection settings"), grid);
  const adjust = () => {
    context.textContent = `${kind.input.selectedOptions[0].text} · ${host.input.value}:${port.input.value} · ${kind.input.value === "oracle" ? service.input.value : database.input.value}`;
    service.wrapper.hidden = kind.input.value !== "oracle";
    service.input.required = kind.input.value === "oracle";
    database.wrapper.hidden = kind.input.value === "oracle";
    password.wrapper.hidden = passwordAction.input.value !== "replace";
    password.input.required = passwordAction.input.value === "replace";
  };
  kind.input.onchange = adjust;
  passwordAction.input.onchange = adjust;
  for (const input of [host.input, port.input, database.input, service.input])
    input.oninput = adjust;
  // Reveal invalid advanced fields so native validation can focus them.
  form.addEventListener(
    "invalid",
    () => {
      details.open = true;
    },
    true,
  );
  adjust();
  const notice = element(
    "p",
    "privacy-note",
    "Your password is saved locally and is not sent to your agent.",
  );
  const feedback = element("div", "feedback");
  feedback.setAttribute("role", "status");
  const actions = element("div", "actions");
  const save = button("Save & connect");
  save.type = "submit";
  const cancel = button("Cancel", "button secondary");
  actions.append(cancel, save);
  fields.append(credentials, notice, details, actions);
  form.append(fields, feedback);
  card.append(intro, form);
  root.append(card);
  function update(next: Pick<Setup, "status" | "error" | "datasource_id">) {
    if (signal.aborted) return;
    onStatusChange(next.status);
    status.replaceChildren(statusBadge(next.status));
    fields.disabled = next.status !== "waiting_for_user";
    feedback.className = next.error ? "feedback error" : "feedback";
    feedback.textContent =
      next.error ??
      (next.status === "saving"
        ? "Checking the connection… Required drivers may download on the first connection."
        : next.status === "completed"
          ? "Connected and saved. Return to your agent to continue."
          : next.status === "cancelled"
            ? "Setup cancelled. Nothing was saved."
            : next.status === "expired"
              ? "This request expired. Ask your agent to open a new setup page."
              : "");
    if (next.status === "completed" && next.datasource_id) {
      const link = element("a", "saved-datasource-link", "View datasource");
      link.href = `/datasource/${next.datasource_id}`;
      feedback.append(" ", link);
    }
  }
  async function poll(): Promise<void> {
    if (signal.aborted) return;
    try {
      const next = await api<Setup>(`/setups/${id}/status`, undefined, signal);
      update(next);
      if (next.status === "saving") pollLater(signal, poll);
    } catch (error) {
      if (signal.aborted) return;
      feedback.className = "feedback error";
      feedback.textContent = message(error);
    }
  }
  form.onsubmit = async (event) => {
    event.preventDefault();
    if (signal.aborted) return;
    if (!form.reportValidity()) return;
    const connection = {
      database_type: kind.input.value,
      host: host.input.value,
      port: Number(port.input.value),
      database: database.input.value,
      service: service.input.value,
      username: username.input.value,
      password: password.input.value,
      tls: tls.input.value,
      properties: {},
    };
    fields.disabled = true;
    feedback.className = "feedback";
    feedback.textContent = "Checking the connection…";
    try {
      const next = await api<Setup>(`/setups/${id}`, {
        name: name.input.value,
        connection,
        password_action: passwordAction.input.value,
      });
      password.input.value = "";
      update(next);
      if (next.status === "saving") void poll();
    } catch (error) {
      if (signal.aborted) return;
      password.input.value = "";
      fields.disabled = false;
      feedback.className = "feedback error";
      feedback.textContent = message(error);
    } finally {
      connection.password = "";
    }
  };
  cancel.onclick = async () => {
    if (signal.aborted) return;
    fields.disabled = true;
    try {
      update(await api<Setup>(`/setups/${id}/cancel`, {}));
    } catch (error) {
      if (signal.aborted) return;
      fields.disabled = false;
      feedback.className = "feedback error";
      feedback.textContent = message(error);
    }
  };
  update(setup);
  if (setup.status === "saving") void poll();
  else if (setup.status === "waiting_for_user")
    (passwordAction.input.value === "replace" && username.input.value
      ? password.input
      : username.input
    ).setAttribute("data-initial-focus", "");
}
