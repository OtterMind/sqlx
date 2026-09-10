import { api } from "./api";
import {
  button,
  element,
  field,
  heading,
  message,
  selectField,
  statusBadge,
} from "./components";

interface Connection {
  database_type: string;
  host: string;
  port: number;
  database: string;
  service: string;
  username: string;
  tls: string;
}
interface Setup {
  request_id: string;
  name: string;
  connection: Connection;
  editing: boolean;
  status: string;
  error: string | null;
  datasource_id: string | null;
}

export async function setupPage(root: HTMLElement, id: string): Promise<void> {
  const setup = await api<Setup>(`/setups/${id}`);
  root.replaceChildren(
    heading(
      "CONNECT A DATASOURCE",
      setup.editing ? "Update your connection" : "One last step to connect",
      "Your connection details are ready. Add your credentials below to continue.",
    ),
  );
  const layout = element("div", "setup-layout");
  const card = element("section", "card setup-card");
  const form = element("form");
  const intro = element("div", "card-heading");
  const status = element("div");
  status.append(statusBadge(setup.status));
  intro.append(element("h2", "", "Database credentials"), status);
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
    passwordAction.wrapper,
  );
  const credentials = element("div", "field-grid");
  credentials.append(username.wrapper, password.wrapper);
  const details = element("details", "connection-details");
  details.append(
    element(
      "summary",
      "",
      `${kind.input.selectedOptions[0].text} · ${setup.connection.host}:${setup.connection.port} · ${setup.connection.database || setup.connection.service}`,
    ),
    grid,
  );
  if (setup.editing) details.open = true;
  const adjust = () => {
    service.wrapper.hidden = kind.input.value !== "oracle";
    service.input.required = kind.input.value === "oracle";
    database.wrapper.hidden = kind.input.value === "oracle";
    password.wrapper.hidden = passwordAction.input.value !== "replace";
    password.input.required = passwordAction.input.value === "replace";
  };
  kind.input.onchange = adjust;
  passwordAction.input.onchange = adjust;
  adjust();
  const notice = element(
    "p",
    "privacy-note",
    "Your password is sent directly to the local SQLX service. It is not returned to your agent.",
  );
  const feedback = element("div", "feedback");
  feedback.setAttribute("role", "status");
  const actions = element("div", "actions");
  const save = button("Save & connect");
  save.type = "submit";
  const cancel = button("Cancel", "button secondary");
  actions.append(save, cancel);
  fields.append(details, credentials, notice, actions);
  form.append(fields, feedback);
  card.append(intro, form);
  const aside = element("aside", "aside");
  aside.append(
    element("div", "aside-icon", "↗"),
    element("h2", "", "Ready for your agent"),
    element(
      "p",
      "",
      "Once saved, your agent can use this connection without asking for your password.",
    ),
    element("hr"),
    element("h3", "", "What happens next"),
    element(
      "p",
      "",
      "SQLX checks the connection, encrypts your saved credentials, and marks this setup complete.",
    ),
  );
  layout.append(card, aside);
  root.append(layout);
  function update(next: Pick<Setup, "status" | "error" | "datasource_id">) {
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
  }
  async function poll(): Promise<void> {
    try {
      const next = await api<Setup>(`/setups/${id}/status`);
      update(next);
      if (next.status === "saving") setTimeout(() => void poll(), 800);
    } catch (error) {
      feedback.className = "feedback error";
      feedback.textContent = message(error);
    }
  }
  form.onsubmit = async (event) => {
    event.preventDefault();
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
      password.input.value = "";
      fields.disabled = false;
      feedback.className = "feedback error";
      feedback.textContent = message(error);
    } finally {
      connection.password = "";
    }
  };
  cancel.onclick = async () => {
    fields.disabled = true;
    try {
      update(await api<Setup>(`/setups/${id}/cancel`, {}));
    } catch (error) {
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
    ).focus();
}
