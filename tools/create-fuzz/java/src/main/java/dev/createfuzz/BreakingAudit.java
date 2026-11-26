package dev.createfuzz;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.simibubi.create.AllTags;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.TagKey;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameRules;
import net.minecraft.world.level.block.BambooStalkBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.CactusBlock;
import net.minecraft.world.level.block.FallingBlock;
import net.minecraft.world.level.block.KelpBlock;
import net.minecraft.world.level.block.KelpPlantBlock;
import net.minecraft.world.level.block.SugarCaneBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.IntegerProperty;
import net.minecraft.world.level.block.state.properties.Property;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;

/** Checks the driver's vanilla blocks (`FUZZ_BREAKING_MANIFEST`) against the loaded game, as a
 * breaker meets them: hardness, tags, the classes `SawBlockEntity` and `TreeCutter` ask about,
 * and what `Block.getDrops` gives without a tool or a player. Every disagreement is reported. */
final class BreakingAudit {
	private static final Set<String> IDS = new TreeSet<>();

	/** Whether `id` is one of the catalogue's blocks: the driver keeps its rules. */
	static boolean dry(String id) {
		synchronized (IDS) {
			if (IDS.isEmpty()) document().getAsJsonArray("blocks").forEach(e -> IDS.add(e.getAsJsonObject().get("id").getAsString()));
			return IDS.contains(id);
		}
	}

	private static JsonObject document() {
		try {
			return JsonParser.parseString(Files.readString(Path.of(System.getenv("FUZZ_BREAKING_MANIFEST")))).getAsJsonObject();
		} catch (IOException e) {throw new IllegalStateException(e);}
	}

	static void check(ServerLevel level, MinecraftServer server) {
		JsonObject doc = document();
		List<String> failures = new ArrayList<>();
		BlockPos probe = new BlockPos(0, 40, 0);
		level.setChunkForced(0, 0, true);
		level.getChunk(0, 0);
		for (JsonElement element : doc.getAsJsonArray("blocks")) {
			JsonObject b = element.getAsJsonObject();
			String id = b.get("id").getAsString();
			String failed = "breaking contract: " + id;
			if (!IDS.add(id)) failures.add(failed + " is listed twice");
			BlockState state;
			try {state = BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK.asLookup(), id, false).blockState();}
			catch (Exception ex) {failures.add(failed + ": " + ex);continue;}
			JsonObject properties = b.has("properties") ? b.getAsJsonObject("properties") : new JsonObject();
			if (properties.size() != state.getProperties().size()) failures.add(failed + " properties " + state.getProperties().stream().map(Property::getName).toList() + " not " + properties.keySet());
			for (Property<?> property : state.getProperties()) {
				if (!properties.has(property.getName())) continue;
				Set<String> expected = new TreeSet<>(), actual = new TreeSet<>();
				properties.getAsJsonArray(property.getName()).forEach(v -> expected.add(v.getAsString()));
				for (Object value : property.getPossibleValues()) actual.add(valueName(property, value));
				if (!expected.equals(actual)) failures.add(failed + " property " + property.getName() + " " + actual + " not " + expected);
				// The driver fills a state's missing properties with the first value listed.
				String first = properties.has(property.getName()) ? properties.getAsJsonArray(property.getName()).get(0).getAsString() : null;
				String value = valueName(property, state.getValue(property));
				if (!value.equals(first)) failures.add(failed + " property " + property.getName() + " lists " + first + " first; the default is " + value);
			}
			float measured = state.getDestroySpeed(level, probe);
			if (measured != b.get("hardness").getAsFloat()) failures.add(failed + " hardness " + measured + " not " + b.get("hardness"));
			if (state.liquid()) failures.add(failed + " is a fluid");
			for (var tag : doc.getAsJsonObject("tags").entrySet()) {
				boolean member = false;
				for (JsonElement value : tag.getValue().getAsJsonArray()) member |= value.getAsString().equals(id);
				if (state.is(TagKey.create(Registries.BLOCK, ResourceLocation.parse(tag.getKey()))) != member) failures.add(failed + " in " + tag.getKey() + ": " + !member);
			}
			Block block = state.getBlock();
			boolean vertical = block instanceof BambooStalkBlock || block instanceof CactusBlock || block instanceof SugarCaneBlock || block instanceof KelpPlantBlock || block instanceof KelpBlock;
			if (vertical != listed(doc, "vertical", id)) failures.add(failed + " vertical plant: " + vertical);
			if (flag(b, "falls") != (block instanceof FallingBlock)) failures.add(failed + " falling: " + (block instanceof FallingBlock));
			boolean leaf = false;
			for (Property<?> property : state.getProperties())
				leaf |= property instanceof IntegerProperty ip && property.getName().equals("distance") && property != BlockStateProperties.STABILITY_DISTANCE;
			if (flag(b, "leaf") != leaf) failures.add(failed + " leaf: " + leaf);
			if (b.has("stands_on")) {
				if (state.canSurvive(level, probe)) failures.add(failed + " stands on air");
				for (JsonElement ground : b.getAsJsonArray("stands_on")) {
					level.setBlockAndUpdate(probe.below(), parse(ground.getAsString()));
					if (!state.canSurvive(level, probe)) failures.add(failed + " does not stand on " + ground.getAsString());
					level.setBlockAndUpdate(probe.below(), Blocks.AIR.defaultBlockState());
				}
			}
			failures.addAll(loot(level, probe, state, b, failed));
		}
		for (var tag : doc.getAsJsonObject("tags").entrySet())
			for (JsonElement value : tag.getValue().getAsJsonArray())
				if (!IDS.contains(value.getAsString())) failures.add("breaking contract: " + tag.getKey() + " names a block outside the catalogue: " + value);
		for (var holder : server.registryAccess().registryOrThrow(Registries.BLOCK).getTagOrEmpty(AllTags.AllBlockTags.NON_BREAKABLE.tag))
			failures.add("breaking contract: create:non_breakable is bound here (" + holder.unwrapKey().orElseThrow() + "), the driver assumes it empty");
		if (!level.getGameRules().getBoolean(GameRules.RULE_DOBLOCKDROPS)) failures.add("breaking contract: doTileDrops starts off");
		if (!failures.isEmpty()) throw new IllegalStateException(String.join("; ", failures));
	}

	private static BlockState parse(String id) {
		try {return BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK.asLookup(), id, false).blockState();}
		catch (Exception ex) {throw new IllegalStateException("breaking contract: " + id, ex);}
	}

	@SuppressWarnings({"rawtypes", "unchecked"})
	private static String valueName(Property property, Object value) {
		return property.getName((Comparable) value);
	}

	private static boolean flag(JsonObject b, String key) {
		return b.has(key) && b.get(key).getAsBoolean();
	}

	private static boolean listed(JsonObject doc, String key, String id) {
		for (JsonElement value : doc.getAsJsonArray(key)) if (value.getAsString().equals(id)) return true;
		return false;
	}

	/** Fixed loot gives the listed stacks every time; rolled loot gives only the listed items, each
	 * of them at least once, in at least two ways. */
	private static List<String> loot(ServerLevel level, BlockPos probe, BlockState state, JsonObject b, String failed) {
		List<String> failures = new ArrayList<>();
		boolean rolled = b.has("rolled");
		if (rolled == b.has("drops")) return List.of(failed + " drops " + (rolled ? "both fixed and rolled" : "neither fixed nor rolled") + " loot");
		TreeMap<String, Integer> want = new TreeMap<>();
		if (!rolled) for (JsonElement element : b.getAsJsonArray("drops")) {
			var drop = element.getAsJsonObject();
			want.put(drop.get("item").getAsString(), drop.get("count").getAsInt());
		}
		Set<String> items = new TreeSet<>(), seen = new TreeSet<>(), outcomes = new TreeSet<>(), unlisted = new TreeSet<>();
		if (rolled) for (JsonElement element : b.getAsJsonArray("rolled")) items.add(element.getAsString());
		TreeMap<String, Integer> first = null;
		for (int i = 0; i < (rolled ? 4096 : 64); i++) {
			TreeMap<String, Integer> got = new TreeMap<>();
			for (ItemStack stack : Block.getDrops(state, level, probe, null, null, ItemStack.EMPTY))
				if (!stack.isEmpty()) got.merge(BuiltInRegistries.ITEM.getKey(stack.getItem()).toString(), stack.getCount(), Integer::sum);
			if (!rolled && !got.equals(want) && first == null) first = got;
			for (String item : got.keySet()) {
				if (rolled && !items.contains(item)) unlisted.add(item);
				seen.add(item);
			}
			outcomes.add(got.toString());
		}
		if (first != null) failures.add(failed + " drops " + first + " not " + want);
		if (rolled && !unlisted.isEmpty()) failures.add(failed + " drops " + unlisted + ", which are not listed");
		if (rolled && !seen.equals(items)) {
			Set<String> missing = new TreeSet<>(items);
			missing.removeAll(seen);
			failures.add(failed + " never drops " + missing + " in 4096 tries");
		}
		if (rolled && outcomes.size() < 2) failures.add(failed + " always drops " + outcomes);
		return failures;
	}
}
