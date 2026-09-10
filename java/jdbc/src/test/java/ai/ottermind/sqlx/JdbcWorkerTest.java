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
    }
}
