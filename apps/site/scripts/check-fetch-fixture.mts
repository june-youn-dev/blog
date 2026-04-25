import {
  assertFixtureFetchOutput,
  permalinksDir,
  postsDir,
  runFetchPosts,
  withFixtureApi,
  withPreservedDirectories,
} from "./fixture-utils.mts";

await withPreservedDirectories([postsDir, permalinksDir], async () => {
  await withFixtureApi(async (apiUrl) => {
    await runFetchPosts(apiUrl);
    await assertFixtureFetchOutput();
  });
});
