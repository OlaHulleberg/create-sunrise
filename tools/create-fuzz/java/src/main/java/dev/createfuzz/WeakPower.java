package dev.createfuzz;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.simibubi.create.content.contraptions.chassis.StickerBlock;
import com.simibubi.create.content.contraptions.elevator.ElevatorContactBlock;
import com.simibubi.create.content.kinetics.base.AbstractEncasedShaftBlock;
import com.simibubi.create.content.kinetics.chainDrive.ChainDriveBlock;
import com.simibubi.create.content.kinetics.transmission.sequencer.SequencedGearshiftBlock;
import com.simibubi.create.content.logistics.packager.PackagerBlock;
import com.simibubi.create.content.logistics.redstoneRequester.RedstoneRequesterBlock;
import com.simibubi.create.content.redstone.RoseQuartzLampBlock;
import com.simibubi.create.content.redstone.contact.RedstoneContactBlock;
import com.simibubi.create.content.trains.signal.SignalBlock;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashSet;
import java.util.TreeMap;
import java.util.TreeSet;

/**
 * Weak power, as the running binary answers it for every block. A block that is a redstone
 * conductor has the power it receives read through it by the blocks beside it
 * ({@code shouldCheckWeakPower}, {@code SignalGetter.getSignal}) unless it is one of the classes
 * Create overrides it in, which answer no. A block's states must agree; one whose do not is
 * "varies". The driver's contract keeps the two apart: its conductor answer, and the overrides
 * as the port lists them.
 */
final class WeakPower {
    static final String CHECKS = "checks", REFUSES = "refuses", CONDUCTS = "conducts", VARIES = "varies";

    /** The classes whose {@code shouldCheckWeakPower} returns false. */
    private static final Class<?>[] OVERRIDES = {
        AbstractEncasedShaftBlock.class, ChainDriveBlock.class, SequencedGearshiftBlock.class,
        RoseQuartzLampBlock.class, RedstoneContactBlock.class, ElevatorContactBlock.class,
        StickerBlock.class, SignalBlock.class, PackagerBlock.class, RedstoneRequesterBlock.class,
    };

    private static boolean overrides(Block block) {
        for (Class<?> c : OVERRIDES) if (c.isInstance(block)) return true;
        return false;
    }

    private static Boolean agreed(Block block, java.util.function.Predicate<BlockState> answer) {
        Boolean all = null;
        for (BlockState state : block.getStateDefinition().getPossibleStates()) {
            boolean c = answer.test(state);
            if (all == null) all = c;
            else if (all != c) return null;
        }
        return all;
    }

    private static Boolean conducts(Block block, ServerLevel level) {
        return agreed(block, state -> state.isRedstoneConductor(level, BlockPos.ZERO));
    }

    private static Boolean checks(Block block, ServerLevel level) {
        Boolean all = null;
        for (BlockState state : block.getStateDefinition().getPossibleStates())
            for (Direction side : Direction.values()) {
                boolean c = state.shouldCheckWeakPower(level, BlockPos.ZERO, side);
                if (all == null) all = c;
                else if (all != c) return null;
            }
        return all;
    }

    private static String name(Boolean answer) {
        return answer == null ? VARIES : answer ? CHECKS : REFUSES;
    }

    private static String conductor(Boolean answer) {
        return answer == null ? VARIES : answer ? CONDUCTS : REFUSES;
    }

    /** What the game answers for a block: its conductor answer, unless Create overrides it. */
    private static String composed(Block block, ServerLevel level) {
        if (overrides(block)) return REFUSES;
        Boolean conducts = conducts(block, level);
        return conducts == null ? VARIES : conducts ? CHECKS : REFUSES;
    }

    private static final String[] OWN = {"minecraft:air", "minecraft:stone", "minecraft:redstone_block", "createfuzz:signal"};

    /** Create's blocks and the scenarios' own vanilla blocks: each one's conductor answer, and Create's overrides by id. */
    static JsonObject json(ServerLevel level) {
        var answers = new TreeMap<String, String>();
        var overrides = new TreeSet<String>();
        for (var block : BuiltInRegistries.BLOCK) {
            var id = BuiltInRegistries.BLOCK.getKey(block);
            if (id.getNamespace().equals("create") && overrides(block)) overrides.add(id.toString());
            if (!id.getNamespace().equals("create")) continue;
            answers.put(id.toString(), conductor(conducts(block, level)));
        }
        for (String own : OWN)
            answers.put(own, conductor(conducts(BuiltInRegistries.BLOCK.get(ResourceLocation.parse(own)), level)));
        var blocks = new JsonObject();
        answers.forEach(blocks::addProperty);
        var contract = new JsonObject();
        contract.add("blocks", blocks);
        contract.add("overrides", new GsonBuilder().create().toJsonTree(overrides));
        return contract;
    }

    static String pretty(JsonObject o) {
        return new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(o) + "\n";
    }

    /** The driver's contract against the binary: every Create block's conductor answer and override. */
    static void audit(ServerLevel level) throws java.io.IOException {
        var contract = JsonParser.parseString(Files.readString(Path.of(System.getenv("FUZZ_WEAK_POWER_MANIFEST")))).getAsJsonObject();
        var blocks = contract.getAsJsonObject("blocks");
        var overrides = new HashSet<String>();
        for (var e : contract.getAsJsonArray("overrides")) overrides.add(e.getAsString());
        for (var id : overrides)
            if (!BuiltInRegistries.BLOCK.containsKey(ResourceLocation.parse(id)))
                throw new IllegalStateException("weak power: unknown block among the overrides: " + id);
        for (var block : BuiltInRegistries.BLOCK) {
            var id = BuiltInRegistries.BLOCK.getKey(block);
            boolean overridden = overrides(block);
            if (overridden != overrides.contains(id.toString()))
                throw new IllegalStateException("weak power: override differs: " + id + " contract " + overrides.contains(id.toString()) + " binary " + overridden);
            var entry = blocks.get(id.toString());
            if (entry == null) {
                if (id.getNamespace().equals("create"))
                    throw new IllegalStateException("weak power: Create's block is not in the contract: " + id);
                continue;
            }
            if (!entry.getAsString().equals(conductor(conducts(block, level))))
                throw new IllegalStateException("weak power: conductor differs: " + id + " contract " + entry);
            // The answer the game gives is what the driver composes: its conductor answer, or no.
            if (!name(checks(block, level)).equals(composed(block, level)))
                throw new IllegalStateException("weak power: shouldCheckWeakPower differs: " + id + " binary " + name(checks(block, level)) + " composed " + composed(block, level));
        }
        for (var id : blocks.keySet())
            if (!BuiltInRegistries.BLOCK.containsKey(ResourceLocation.parse(id)))
                throw new IllegalStateException("weak power: unknown block in the contract: " + id);
    }
}
