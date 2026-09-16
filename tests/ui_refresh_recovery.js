// Browser operations run through playwright-cli; Python owns the DB outage.
async (page) => {
  const config = __CONFIG__;
  const assert = (value, message) => { if (!value) throw Error(message); };
  const button = (name) => page.getByRole('button', {name, exact:true});
  const feedback = page.locator('.refresh-feedback');
  const caption = page.locator('.refresh-main span');
  const idle = () => page.waitForFunction(() => {
    const refresh = document.querySelector('[aria-label="Refresh data"]');
    return refresh && !refresh.disabled;
  });
  const cached = async () => {
    const values = await page.locator('.table-viewport tbody td').allTextContents();
    assert(JSON.stringify(values) === JSON.stringify(['9007199254740993','retained']), 'Cached values changed');
    assert(!await page.locator('.connection-notice').isVisible(), 'Database outage misreported as SQLX outage');
  };
  const interval = async (seconds) => {
    await button('Auto refresh interval').click();
    await page.getByRole('menuitemradio', {name:seconds ? `Every ${seconds}s` : 'Manual refresh', exact:true}).click();
  };
  page.setDefaultTimeout(10000);
  const cases = {
    async initial() {
      await page.goto(config.url);
      await page.locator('.table-viewport tbody tr').waitFor();
      await idle(); await cached();
      assert(!await feedback.isVisible(), 'Fresh result has an error');
    },
    async outage() {
      await interval(5);
      await feedback.filter({hasText:'Refresh failed:'}).waitFor();
      const text = await feedback.innerText();
      assert(text.includes('MySQL at '+config.endpoint+' refused the connection'), text);
      assert(!text.includes('Input/output error'), 'Nested driver error exposed');
      assert(!await caption.isVisible(), 'Automatic refresh still active');
      await idle(); await cached();
      // The timer must stop after this failure rather than reconnect in a loop.
      let retried = false;
      const listener = request => { if (request.method() === 'POST') retried = true; };
      page.on('request', listener);
      await page.waitForTimeout(5200);
      page.off('request', listener);
      assert(!retried, 'SQL automatically retried after failure');
    },
    async historical() {
      await page.reload(); await idle();
      await feedback.filter({hasText:'Previous refresh failed:'}).waitFor();
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await cached();
    },
    async recover() {
      await button('Refresh data').click();
      await idle(); await cached();
      assert(!await feedback.isVisible(), 'Successful refresh kept error');
      assert(!await caption.isVisible(), 'Recovery enabled auto refresh without selection');
    },
    async second_outage() {
      await button('Refresh data').click();
      await feedback.filter({hasText:'Refresh failed:'}).waitFor();
      await idle(); await cached();
    },
    async recover_on_focus() {
      // Another page successfully refreshes the same result. Returning to the
      // first page should clear its stale error without executing SQL again.
      const {origin,path,request_id} = await page.evaluate(() => ({
        origin:location.origin,
        path:location.pathname.replace('/result/','/api/results/'),
        request_id:crypto.randomUUID()
      }));
      const response = await page.request.post(origin+path+'/refresh', {
        headers:{'X-SQLX-UI':'1','Origin':origin}, data:{request_id}
      });
      assert(response.ok(), 'Recovery request rejected');
      let meta;
      for (let i=0;i<100;i++) {
        meta = await (await page.request.get(origin+path, {headers:{'X-SQLX-UI':'1'}})).json();
        if (meta.refresh.status !== 'running') break;
        await page.waitForTimeout(50);
      }
      assert(meta.refresh.status === 'completed', 'Recovery did not finish');
      assert(await feedback.isVisible(), 'Fixture has no stale feedback');
      let writes = 0;
      const listener = request => { if (request.method() === 'POST') writes++; };
      page.on('request', listener);
      const synced = page.waitForResponse(response => response.url() === origin+path);
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await synced;
      await feedback.waitFor({state:'hidden'}); await idle(); await cached();
      page.off('request', listener);
      assert(writes === 0, 'Focus replayed SQL');
    },
    async auto_refresh_recovery() {
      const response = page.waitForResponse(response => response.url().endsWith('/refresh'));
      await interval(5); await response; await idle();
      assert(await caption.innerText() === '5s', 'Successful timer lost enabled state');
      assert(!await feedback.isVisible(), 'Successful timer showed error');
      await interval(0); await cached();
    },
    async restart() {
      const errors = [];
      const listener = error => errors.push(String(error));
      page.on('pageerror', listener);
      await page.reload(); await idle();
      await page.locator('.table-viewport tbody tr').waitFor(); await cached();
      assert(!await feedback.isVisible(), 'Restart restored stale error');
      assert(!await caption.isVisible(), 'Restart automatically reran SQL');
      page.off('pageerror', listener);
      assert(!errors.length, errors.join('\n'));
    }
  };
  await cases[config.case]();
  return {passed:config.case};
}
