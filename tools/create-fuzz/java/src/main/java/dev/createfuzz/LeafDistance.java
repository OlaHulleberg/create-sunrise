package dev.createfuzz;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.state.BlockState;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.TreeSet;

/**
 * Leaf distances, as the running binary cascades them. The driver's contract
 * ({@code FUZZ_LEAF_MANIFEST}) is replayed through the case runner at startup, and every leaf
 * of a shape is read after each level tick: its {@code distance}, and whether a tick of the leaf
 * there is pending. Each is a timeline, a digit or a mark per tick, compared with the contract's.
 * The measurement is written to {@code leaf-distances.json} before it is held to the contract.
 */
final class LeafDistance {
    static final String MEASURED = "leaf-distances.json";

    private static JsonObject document() {
        String manifest = System.getenv("FUZZ_LEAF_MANIFEST");
        if (manifest == null || manifest.isBlank())
            throw new IllegalStateException("leaf distances: start the harness with run.sh server");
        try {
            return JsonParser.parseString(Files.readString(Path.of(manifest))).getAsJsonObject();
        } catch (IOException e) {
            throw new IllegalStateException("leaf distances: no contract at " + manifest, e);
        }
    }

    /** The contract's shapes as a batch, every tick a checkpoint and every block asked for its pending tick. */
    static CreateFuzz.Batch start(ServerLevel level, Path dir) {
        JsonArray cases = new JsonArray();
        for (JsonElement e : document().getAsJsonArray("fixtures")) {
            JsonObject c = e.getAsJsonObject().getAsJsonObject("scenario").deepCopy();
            JsonArray checkpoints = new JsonArray();
            for (int t = 0; t <= c.get("ticks").getAsInt(); t++) checkpoints.add(t);
            c.add("checkpoints", checkpoints);
            c.addProperty("pending_ticks", true);
            cases.add(c);
        }
        CreateFuzz.Batch batch = new CreateFuzz.Batch("leaf-audit", cases, level, dir);
        batch.start(level);
        return batch;
    }

    /** Writes the measurement, then holds it to the contract. */
    static void done(Path dir, CreateFuzz.Batch batch) throws IOException {
        JsonArray contract = document().getAsJsonArray("fixtures");
        JsonArray measured = new JsonArray();
        List<String> problems = new ArrayList<>();
        for (int i = 0; i < contract.size(); i++) {
            JsonObject fixture = contract.get(i).getAsJsonObject();
            String name = fixture.get("name").getAsString();
            JsonObject leaves = trace(batch.results.get(i).getAsJsonObject(), fixture.getAsJsonObject("scenario"));
            JsonObject one = new JsonObject();
            one.addProperty("name", name);
            one.add("leaves", leaves);
            measured.add(one);
            JsonObject want = fixture.getAsJsonObject("leaves");
            TreeSet<String> keys = new TreeSet<>(want.keySet());
            keys.addAll(leaves.keySet());
            for (String key : keys) {
                if (!want.has(key) || !leaves.has(key) || !want.get(key).equals(leaves.get(key)))
                    problems.add(name + " leaf " + key + ": contract " + want.get(key) + ", server " + leaves.get(key));
            }
        }
        Files.writeString(dir.resolve(MEASURED),
                new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(measured) + "\n");
        if (!problems.isEmpty())
            throw new IllegalStateException("leaf distances:\n" + String.join("\n", problems));
    }

    /**
     * Every position the scenario gives a leaf, by {@code x,y,z}: after each tick and at the
     * end, a digit for the distance of the leaf there and a mark for a pending tick of it, a dot
     * while another block is there.
     */
    static JsonObject trace(JsonObject result, JsonObject scenario) {
        TreeSet<String> positions = new TreeSet<>();
        for (String list : new String[] {"blocks", "events"}) {
            if (!scenario.has(list)) continue;
            for (JsonElement e : scenario.getAsJsonArray(list)) {
                JsonObject b = e.getAsJsonObject();
                if (b.has("state") && state(b.get("state").getAsString()).getBlock() instanceof LeavesBlock) {
                    JsonArray p = b.getAsJsonArray("pos");
                    positions.add(p.get(0).getAsInt() + "," + p.get(1).getAsInt() + "," + p.get(2).getAsInt());
                }
            }
        }
        int ticks = scenario.get("ticks").getAsInt();
        JsonObject out = new JsonObject();
        for (String key : positions) {
            StringBuilder distance = new StringBuilder(), pending = new StringBuilder();
            for (int t = 0; t <= ticks; t++) {
                JsonObject at = t == ticks ? result : result.getAsJsonObject("checkpoints").getAsJsonObject(Integer.toString(t));
                JsonObject block = at.getAsJsonObject("blocks").getAsJsonObject(key);
                if (block == null) throw new IllegalStateException("leaf distances: the snapshot has no " + key);
                BlockState state = state(block.get("state").getAsString());
                if (state.getBlock() instanceof LeavesBlock) {
                    distance.append(state.getValue(LeavesBlock.DISTANCE));
                    pending.append(block.has("tick_pending") ? '*' : '.');
                } else {
                    distance.append('.');
                    pending.append('.');
                }
            }
            JsonArray timelines = new JsonArray();
            timelines.add(distance.toString());
            timelines.add(pending.toString());
            out.add(key, timelines);
        }
        return out;
    }

    private static BlockState state(String id) {
        try {
            return BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK.asLookup(), id, false).blockState();
        } catch (Exception ex) {
            throw new IllegalStateException("leaf distances: bad state " + id, ex);
        }
    }
}
