package ai.ottermind.sqlx;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import java.io.*;
import java.sql.*;
import static org.junit.jupiter.api.Assertions.*;

class JdbcWorkerTest {
    @Test void completeValuesAndDuplicateLabels() throws Exception {
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();
        JdbcWorker worker=new JdbcWorker(new PrintStream(bytes,true,"UTF-8"));
        try(Connection c=DriverManager.getConnection("jdbc:h2:mem:codec");Statement st=c.createStatement();ResultSet rs=st.executeQuery("SELECT CAST(9007199254740993 AS BIGINT) AS DUP, CAST(123.4500 AS DECIMAL(30,4)) AS DUP, X'00FF' AS BINARY_VALUE, REPEAT('x', 1100000) AS LARGE_VALUE, CAST(NULL AS VARCHAR) AS NULL_VALUE")) {
            worker.rows(rs,0,0);
        }
        var lines=bytes.toString("UTF-8").lines().toList();
        ObjectMapper json=new ObjectMapper();
        var columns=json.readTree(lines.get(0)).path("columns");
        assertEquals(columns.get(0).path("name"),columns.get(1).path("name"));
        var values=json.readTree(lines.get(1)).path("values");
        assertEquals("9007199254740993",values.get(0).asText());
        assertEquals("123.4500",values.get(1).asText());
        assertEquals("AP8=",values.get(2).asText());
        assertEquals(1100000,values.get(3).asText().length());
        assertTrue(values.get(4).isNull());
        assertEquals("result_end",json.readTree(lines.get(2)).path("event").asText());
    }
    @Test void urlsSelectVendorAndTransport() throws Exception {
        ObjectMapper json=new ObjectMapper();
        var oracle=json.readTree("{\"database_type\":\"oracle\",\"host\":\"localhost\",\"port\":1521,\"service\":\"FREEPDB1\",\"tls\":\"disable\"}");
        assertEquals("jdbc:oracle:thin:@tcp://localhost:1521/FREEPDB1",JdbcWorker.url(oracle));
        var hive=json.readTree("{\"database_type\":\"hive\",\"host\":\"localhost\",\"port\":10000,\"database\":\"default\",\"tls\":\"disable\"}");
        assertEquals("jdbc:hive2://localhost:10000/default",JdbcWorker.url(hive));
        var presto=json.readTree("{\"database_type\":\"presto\",\"host\":\"localhost\",\"port\":8080,\"database\":\"tpch.tiny\",\"tls\":\"disable\"}");
        assertEquals("jdbc:presto://localhost:8080/tpch/tiny",JdbcWorker.url(presto));
        var kylin=json.readTree("{\"database_type\":\"kylin\",\"host\":\"localhost\",\"port\":7070,\"database\":\"learn_kylin\",\"tls\":\"disable\"}");
        assertEquals("jdbc:kylin://localhost:7070/learn_kylin",JdbcWorker.url(kylin));
        var xugu=json.readTree("{\"database_type\":\"xugu\",\"host\":\"localhost\",\"port\":5138,\"database\":\"SYSTEM\",\"tls\":\"disable\"}");
        assertEquals("jdbc:xugu://localhost:5138/SYSTEM",JdbcWorker.url(xugu));
        var db2=json.readTree("{\"database_type\":\"db2\",\"host\":\"localhost\",\"port\":50000,\"database\":\"testdb\",\"tls\":\"disable\"}");
        assertEquals("jdbc:db2://localhost:50000/testdb",JdbcWorker.url(db2));
        var sundb=json.readTree("{\"database_type\":\"sundb\",\"host\":\"localhost\",\"port\":22581,\"database\":\"sundb\",\"tls\":\"disable\"}");
        assertEquals("jdbc:goldilocks://localhost:22581/sundb",JdbcWorker.url(sundb));
        // Informix and GBase 8s refuse a URL without a server instance and take it from --service.
        var informix=json.readTree("{\"database_type\":\"informix\",\"host\":\"localhost\",\"port\":9088,\"database\":\"sysmaster\",\"service\":\"informix\",\"tls\":\"disable\"}");
        assertEquals("jdbc:informix-sqli://localhost:9088/sysmaster:INFORMIXSERVER=informix;",JdbcWorker.url(informix));
        var gbase=json.readTree("{\"database_type\":\"gbase8s\",\"host\":\"localhost\",\"port\":9088,\"database\":\"sysmaster\",\"service\":\"gbase01\",\"tls\":\"disable\"}");
        assertEquals("jdbc:gbasedbt-sqli://localhost:9088/sysmaster:GBASEDBTSERVER=gbase01;",JdbcWorker.url(gbase));
        var informixTls=json.readTree("{\"database_type\":\"informix\",\"host\":\"localhost\",\"port\":9088,\"database\":\"sysmaster\",\"tls\":\"verify-full\"}");
        assertEquals("jdbc:informix-sqli://localhost:9088/sysmaster:sslConnection=true;",JdbcWorker.url(informixTls));
    }
    @Test void anEngineWithoutAUrlArmIsRejected() throws Exception {
        ObjectMapper json=new ObjectMapper();
        var unknown=json.readTree("{\"database_type\":\"mongodb\",\"host\":\"localhost\",\"port\":27017,\"tls\":\"disable\"}");
        // The message names every engine the worker can reach, so a new arm is never silently missing.
        IllegalArgumentException failure=assertThrows(IllegalArgumentException.class,()->JdbcWorker.url(unknown));
        assertTrue(failure.getMessage().contains("db2"),failure.getMessage());
        assertTrue(failure.getMessage().contains("gbase8s"),failure.getMessage());
    }
}
