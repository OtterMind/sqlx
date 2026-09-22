package ai.ottermind.sqlx;

// External class loading and session ownership adapted from Chat2DB-Rust. See NOTICE.
import com.fasterxml.jackson.core.StreamReadConstraints;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.*;
import java.net.*;
import java.nio.file.*;
import java.sql.*;
import java.util.*;
import java.util.logging.Level;
import java.util.logging.LogManager;
import java.util.logging.Logger;

public final class JdbcWorker {
    private static final ObjectMapper JSON = new ObjectMapper();
    static {
        JSON.getFactory().setStreamReadConstraints(StreamReadConstraints.builder().maxStringLength(Integer.MAX_VALUE).build());
    }
    private final PrintStream out;
    JdbcWorker(PrintStream out) { this.out = out; }
    /**
     * Vendor drivers log connection diagnostics through java.util.logging, which floods
     * stderr with dozens of lines when a connection fails. Keep it quiet unless
     * SQLX_JDBC_DEBUG is set.
     */
    private static void quietDriverLogging() {
        if (System.getenv("SQLX_JDBC_DEBUG") != null) return;
        LogManager.getLogManager().reset();
        Logger.getLogger("").setLevel(Level.OFF);
    }
    public static void main(String[] args) {
        quietDriverLogging();
        JdbcWorker worker = new JdbcWorker(System.out);
        try {
            worker.emit("ready", "protocol_version", 1);
            JsonNode request = JSON.readTree(System.in);
            boolean ok = worker.run(request);
            if (!ok) System.exit(1);
        } catch (Exception failure) {
            System.err.println("JDBC worker could not complete its protocol response");
            System.exit(1);
        }
    }
    /**
     * Replace standalone occurrences of a secret. A secret that is only part of a word or a
     * hostname stays: the password "oracle" must not turn "docs.oracle.com" into
     * "docs.[redacted].com", and a short username must not mangle unrelated words.
     */
    static String redactSecret(String text, String secret) {
        StringBuilder out = new StringBuilder(text.length());
        int index = 0;
        while (true) {
            int at = text.indexOf(secret, index);
            if (at < 0) { out.append(text, index, text.length()); return out.toString(); }
            int end = at + secret.length();
            boolean standalone = (at == 0 || boundary(text.charAt(at - 1))) && (end == text.length() || boundary(text.charAt(end)));
            out.append(text, index, at).append(standalone ? "[redacted]" : secret);
            index = end;
        }
    }
    private static boolean boundary(char value) { return !(Character.isLetterOrDigit(value) || value == '.'); }
    boolean run(JsonNode request) throws IOException {
        JsonNode config = request.path("connection");
        JsonNode sql = request.path("statements");
        Integer current = null;
        boolean dispatched = false;
        try {
            if (request.path("protocol_version").asInt() != 1) throw new IllegalArgumentException("unsupported protocol version");
            if (!Set.of("test", "execute").contains(request.path("action").asText())) throw new IllegalArgumentException("invalid action");
            URL[] jars = new URL[request.path("driver_jars").size()];
            for (int i=0; i<jars.length; i++) {
                Path p = Path.of(request.path("driver_jars").get(i).asText()).toRealPath();
                if (!Files.isRegularFile(p)) throw new IllegalArgumentException("driver JAR is missing");
                jars[i] = p.toUri().toURL();
            }
            try (URLClassLoader loader = new URLClassLoader(jars, ClassLoader.getPlatformClassLoader())) {
                Class<?> type = Class.forName(request.path("driver_class").asText(), true, loader);
                if (type.getClassLoader() != loader || !Driver.class.isAssignableFrom(type)) throw new IllegalArgumentException("invalid driver class");
                Driver driver = (Driver) type.getDeclaredConstructor().newInstance();
                Properties properties = new Properties();
                config.path("properties").fields().forEachRemaining(e -> properties.setProperty(e.getKey(), e.getValue().asText()));
                String user = config.path("username").asText();
                if (user.isEmpty() && config.path("database_type").asText().equals("h2")) {
                    // H2 always has a user; an embedded database is created for `sa` when none is given.
                    user = "sa";
                }
                properties.setProperty("user", user);
                properties.setProperty("password", config.path("password").asText());
                if (config.path("database_type").asText().equals("sqlserver")) {
                    properties.setProperty("databaseName", config.path("database").asText());
                    properties.setProperty("encrypt", config.path("tls").asText().equals("disable") ? "false" : "true");
                    properties.setProperty("trustServerCertificate", "false");
                    properties.setProperty("loginTimeout", "15");
                }
                Thread.currentThread().setContextClassLoader(loader);
                try (Connection connection = driver.connect(url(config), properties)) {
                    if (connection == null) throw new SQLException("driver rejected connection URL");
                    connection.setAutoCommit(true);
                    if (!connection.isValid(15)) throw new SQLException("connection validation failed");
                    emit("connected");
                    if (request.path("action").asText().equals("execute")) {
                        if (!sql.isArray() || sql.isEmpty()) throw new IllegalArgumentException("SQL statements are required");
                        for (int i=0; i<sql.size(); i++) {
                            current = i;
                            String statement = sql.get(i).asText();
                            if (statement.isBlank()) throw new IllegalArgumentException("empty SQL statement");
                            if (config.path("database_type").asText().equals("h2")) singleStatement(statement);
                            emit("statement_start", "index", i);
                            try (Statement st = connection.createStatement()) {
                                dispatched = true;
                                boolean hasRows = st.execute(statement);
                                int result = 0;
                                while (true) {
                                    if (hasRows) {
                                        try (ResultSet rs = st.getResultSet()) { rows(rs, i, result); }
                                    } else {
                                        long count = updateCount(st);
                                        if (count == -1) break;
                                        emit("columns", "index", i, "result", result, "columns", List.of());
                                        emit("result_end", "index", i, "result", result, "rows", "0", "affected_rows", Long.toString(count));
                                    }
                                    result++;
                                    hasRows = st.getMoreResults(Statement.CLOSE_CURRENT_RESULT);
                                }
                            }
                            emit("statement_end", "index", i);
                            dispatched = false;
                        }
                    }
                    // A caller can issue explicit transaction SQL. Never implicitly commit it here.
                    if (!connection.getAutoCommit()) connection.rollback();
                } finally { Thread.currentThread().setContextClassLoader(ClassLoader.getPlatformClassLoader()); }
            }
            emit("complete", "success", true);
            return true;
        } catch (Exception failure) {
            String code = "jdbc.execution_failed";
            String outcome = dispatched ? "unknown" : "not_started";
            if (failure instanceof SQLException e) {
                code = "jdbc." + Objects.toString(e.getSQLState(), "unknown") + "." + e.getErrorCode();
                if (e.getSQLState() != null && !e.getSQLState().startsWith("08") && !e.getSQLState().startsWith("HYT") && !(e instanceof SQLTimeoutException) && !(e instanceof SQLRecoverableException)) outcome = "failed";
            }
            String message = Objects.toString(failure.getMessage(), "");
            if (message.isBlank()) {
                // A driver may throw without a message; name the class, and its cause, so the failure stays readable.
                message = failure.getClass().getName();
                if (failure.getCause() != null && failure.getCause() != failure) {
                    message += ": " + Objects.toString(failure.getCause().getMessage(), failure.getCause().getClass().getName());
                }
            }
            for (String key : List.of("username", "password")) { String secret=config.path(key).asText(); if (!secret.isEmpty()) message=redactSecret(message, secret); }
            emit("error", "index", current, "code", code, "message", message, "outcome", outcome);
            for (int i=current==null ? 0 : current+1; i<sql.size(); i++) emit("skipped", "index", i);
            emit("complete", "success", false);
            return false;
        }
    }
    /**
     * H2 runs every statement in one string but reports only the first result, so a command that
     * carries more than one statement is rejected before anything runs.
     */
    static void singleStatement(String sql) {
        int terminated = 0;
        boolean text = false;
        char quote = 0;
        for (int i = 0; i < sql.length(); i++) {
            char c = sql.charAt(i);
            if (quote != 0) {
                if (c == quote) {
                    if (i + 1 < sql.length() && sql.charAt(i + 1) == quote) i++;
                    else quote = 0;
                }
                continue;
            }
            if (c == '\'' || c == '"' || c == '`') {
                quote = c;
                text = true;
            } else if (c == '$' && i + 1 < sql.length() && sql.charAt(i + 1) == '$') {
                int end = sql.indexOf("$$", i + 2);
                i = end < 0 ? sql.length() : end + 1;
                text = true;
            } else if (c == '-' && i + 1 < sql.length() && sql.charAt(i + 1) == '-') {
                int end = sql.indexOf('\n', i);
                i = end < 0 ? sql.length() : end;
            } else if (c == '/' && i + 1 < sql.length() && sql.charAt(i + 1) == '*') {
                int end = sql.indexOf("*/", i + 2);
                i = end < 0 ? sql.length() : end + 1;
            } else if (c == ';') {
                if (text) { terminated++; text = false; }
            } else if (!Character.isWhitespace(c)) {
                text = true;
            }
        }
        if (terminated > 0 && text) throw new IllegalArgumentException("one statement per --command; H2 would run the rest without reporting it");
    }
    static String url(JsonNode c) throws URISyntaxException {
        String host=c.path("host").asText(); int port=c.path("port").asInt();
        String authority=host.contains(":") ? "["+host+"]:"+port : host+":"+port;
        return switch (c.path("database_type").asText()) {
            case "oracle" -> "jdbc:oracle:thin:@" + new URI(c.path("tls").asText().equals("disable") ? "tcp" : "tcps", null, host, port, "/"+c.path("service").asText(), null, null).toASCIIString();
            case "sqlserver" -> "jdbc:sqlserver://"+authority;
            case "clickhouse" -> {
                String database = c.path("database").asText();
                yield "jdbc:clickhouse://" + authority + "/" + (database.isEmpty() ? "default" : database)
                    + (c.path("tls").asText().equals("disable") ? "" : "?ssl=true");
            }
            case "dameng" -> "jdbc:dm://" + authority + "/" + c.path("database").asText();
            case "kingbase" -> "jdbc:kingbase8://" + authority + "/" + c.path("database").asText();
            case "opengauss" -> "jdbc:opengauss://" + authority + "/" + c.path("database").asText();
            case "tdengine" -> {
                // The RESTful driver reaches taosAdapter over HTTP, so the native client is not needed.
                String database = c.path("database").asText();
                yield "jdbc:TAOS-WS://" + authority + "/" + database;
            }
            case "h2" -> {
                // An H2 URL either opens a local file or reaches a TCP server; the same driver does
                // both. A server refuses an implicitly relative path, so a bare name becomes ./name.
                String database = c.path("database").asText();
                if (host.isEmpty()) yield "jdbc:h2:file:" + database;
                String remote = database.startsWith("/") || database.startsWith("~") || database.startsWith("./")
                    ? database
                    : "./" + database;
                yield "jdbc:h2:tcp://" + authority + "/" + remote;
            }
            case "trino" -> {
                // Trino addresses a catalog and an optional schema; --database carries catalog[.schema].
                yield "jdbc:trino://" + authority + "/" + c.path("database").asText().replace('.', '/')
                    + (c.path("tls").asText().equals("disable") ? "" : "?SSL=true");
            }
            default -> throw new IllegalArgumentException("JDBC worker supports oracle, sqlserver, clickhouse, trino, tdengine, opengauss, dameng, kingbase and h2");
        };
    }
    /** Some drivers, such as the TDengine RESTful driver, only implement the JDBC 1 update count. */
    static long updateCount(Statement st) throws SQLException {
        try {
            return st.getLargeUpdateCount();
        } catch (SQLException | RuntimeException | Error unsupported) {
            try {
                return st.getUpdateCount();
            } catch (SQLException fallback) {
                if (unsupported instanceof SQLException sql) throw sql;
                throw fallback;
            }
        }
    }
    void rows(ResultSet rs, int index, int result) throws SQLException, IOException {
        ResultSetMetaData md=rs.getMetaData(); int count=md.getColumnCount();
        List<Map<String,Object>> columns=new ArrayList<>();
        for(int i=1;i<=count;i++) columns.add(Map.of("name",md.getColumnLabel(i),"database_type",md.getColumnTypeName(i),"encoding",encoding(md.getColumnType(i))));
        emit("columns","index",index,"result",result,"columns",columns);
        long rows=0;
        while(rs.next()) {
            List<Object> values=new ArrayList<>();
            for(int i=1;i<=count;i++) {
                String encoding=encoding(md.getColumnType(i));
                Object value;
                if(encoding.equals("base64")) { byte[] bytes=rs.getBytes(i); value=bytes==null ? null : Base64.getEncoder().encodeToString(bytes); }
                else if(encoding.equals("boolean")) { boolean b=rs.getBoolean(i); value=rs.wasNull() ? null : b; }
                else value=rs.getString(i);
                values.add(value);
            }
            emit("row","index",index,"result",result,"values",values); rows++;
        }
        emit("result_end","index",index,"result",result,"rows",Long.toString(rows),"affected_rows",null);
    }
    static String encoding(int type) {
        return switch(type) { case Types.BINARY,Types.VARBINARY,Types.LONGVARBINARY,Types.BLOB -> "base64"; case Types.BOOLEAN,Types.BIT -> "boolean"; default -> "string"; };
    }
    void emit(String event,Object... fields) throws IOException {
        Map<String,Object> value=new LinkedHashMap<>(); value.put("event",event);
        for(int i=0;i<fields.length;i+=2) value.put((String)fields[i],fields[i+1]);
        out.println(JSON.writeValueAsString(value)); out.flush();
        if(out.checkError()) throw new IOException("output pipe closed");
    }
}
