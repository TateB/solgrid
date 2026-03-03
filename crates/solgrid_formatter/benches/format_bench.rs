use criterion::{criterion_group, criterion_main, Criterion};
use solgrid_config::FormatConfig;
use solgrid_formatter::format_source;

const SAMPLE_CONTRACT: &str = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/// @title TokenVault
/// @author solgrid-bench
/// @notice A vault for depositing and withdrawing ERC20 tokens
contract TokenVault is Ownable, ReentrancyGuard {
    /// @notice Emitted when tokens are deposited
    /// @param user The depositor
    /// @param token The token address
    /// @param amount The amount deposited
    event Deposit(address indexed user, address indexed token, uint256 amount);

    /// @notice Emitted when tokens are withdrawn
    /// @param user The withdrawer
    /// @param token The token address
    /// @param amount The amount withdrawn
    event Withdrawal(address indexed user, address indexed token, uint256 amount);

    /// @notice Emitted when the fee rate changes
    /// @param oldRate The previous fee rate
    /// @param newRate The new fee rate
    event FeeRateUpdated(uint256 oldRate, uint256 newRate);

    error InsufficientBalance(uint256 requested, uint256 available);
    error ZeroAmount();
    error InvalidToken();
    error FeeTooHigh(uint256 fee);

    struct UserInfo {
        uint256 depositedAmount;
        uint256 lastDepositTime;
        uint256 rewardDebt;
        bool isActive;
    }

    mapping(address => mapping(address => UserInfo)) public userInfo;
    mapping(address => uint256) public totalDeposited;
    mapping(address => bool) public supportedTokens;

    uint256 public constant MAX_FEE_RATE = 1000;
    uint256 public feeRate;
    uint256 public immutable deploymentTime;
    address[] private _supportedTokenList;

    /// @notice Creates the vault
    /// @param initialFeeRate The initial fee rate in basis points
    constructor(uint256 initialFeeRate) Ownable(msg.sender) {
        if (initialFeeRate > MAX_FEE_RATE) {
            revert FeeTooHigh(initialFeeRate);
        }
        feeRate = initialFeeRate;
        deploymentTime = block.timestamp;
    }

    /// @notice Deposit tokens into the vault
    /// @param token The token to deposit
    /// @param amount The amount to deposit
    function deposit(address token, uint256 amount) external nonReentrant {
        if (amount == 0) revert ZeroAmount();
        if (!supportedTokens[token]) revert InvalidToken();

        UserInfo storage info = userInfo[msg.sender][token];
        info.depositedAmount += amount;
        info.lastDepositTime = block.timestamp;
        info.isActive = true;
        totalDeposited[token] += amount;

        IERC20(token).transferFrom(msg.sender, address(this), amount);
        emit Deposit(msg.sender, token, amount);
    }

    /// @notice Withdraw tokens from the vault
    /// @param token The token to withdraw
    /// @param amount The amount to withdraw
    function withdraw(address token, uint256 amount) external nonReentrant {
        if (amount == 0) revert ZeroAmount();

        UserInfo storage info = userInfo[msg.sender][token];
        if (info.depositedAmount < amount) {
            revert InsufficientBalance(amount, info.depositedAmount);
        }

        uint256 fee = (amount * feeRate) / 10000;
        uint256 netAmount = amount - fee;

        info.depositedAmount -= amount;
        if (info.depositedAmount == 0) {
            info.isActive = false;
        }
        totalDeposited[token] -= amount;

        IERC20(token).transfer(msg.sender, netAmount);
        if (fee > 0) {
            IERC20(token).transfer(owner(), fee);
        }
        emit Withdrawal(msg.sender, token, netAmount);
    }

    /// @notice Add a supported token
    /// @param token The token address to support
    function addSupportedToken(address token) external onlyOwner {
        if (token == address(0)) revert InvalidToken();
        if (!supportedTokens[token]) {
            supportedTokens[token] = true;
            _supportedTokenList.push(token);
        }
    }

    /// @notice Update the fee rate
    /// @param newRate The new fee rate in basis points
    function setFeeRate(uint256 newRate) external onlyOwner {
        if (newRate > MAX_FEE_RATE) revert FeeTooHigh(newRate);
        uint256 oldRate = feeRate;
        feeRate = newRate;
        emit FeeRateUpdated(oldRate, newRate);
    }

    /// @notice Get the number of supported tokens
    /// @return The count of supported tokens
    function supportedTokenCount() external view returns (uint256) {
        return _supportedTokenList.length;
    }

    /// @notice Get a supported token by index
    /// @param index The index
    /// @return The token address
    function supportedTokenAt(uint256 index) external view returns (address) {
        return _supportedTokenList[index];
    }

    /// @notice Calculate the fee for a given amount
    /// @param amount The amount
    /// @return The fee amount
    function calculateFee(uint256 amount) public view returns (uint256) {
        return (amount * feeRate) / 10000;
    }
}
"#;

fn bench_format_contract(c: &mut Criterion) {
    let config = FormatConfig::default();
    c.bench_function("format_contract", |b| {
        b.iter(|| format_source(SAMPLE_CONTRACT, &config))
    });
}

fn bench_format_with_options(c: &mut Criterion) {
    let config = FormatConfig {
        single_quote: true,
        bracket_spacing: true,
        sort_imports: true,
        ..FormatConfig::default()
    };
    c.bench_function("format_contract_custom_opts", |b| {
        b.iter(|| format_source(SAMPLE_CONTRACT, &config))
    });
}

fn bench_check_formatted(c: &mut Criterion) {
    let config = FormatConfig::default();
    let formatted = format_source(SAMPLE_CONTRACT, &config).unwrap();
    c.bench_function("check_already_formatted", |b| {
        b.iter(|| solgrid_formatter::check_formatted(&formatted, &config))
    });
}

criterion_group!(
    benches,
    bench_format_contract,
    bench_format_with_options,
    bench_check_formatted
);
criterion_main!(benches);
